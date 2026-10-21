#![cfg(all(
    not(target_family = "wasm"),
    feature = "serde_impl",
    not(feature = "approx-number-parsing")
))]

use hegel::generators as gs;
use hegel::{Generator, TestCase};
use simd_json::prelude::*;
use simd_json::{BorrowedValue, OwnedValue, StaticNode};

#[derive(Debug, Clone, PartialEq)]
enum J {
    Null,
    Bool(bool),
    I(i64),
    U(u64),
    F(u64),
    S(String),
    A(Vec<J>),
    O(Vec<(String, J)>),
}

fn ws(tc: &TestCase) -> String {
    let n = tc.draw_silent(gs::integers::<usize>().max_value(3));
    (0..n)
        .map(|_| tc.draw_silent(gs::sampled_from(vec![' ', '\t', '\n', '\r'])))
        .collect()
}

fn render_str(tc: &TestCase, s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        let mode = tc.draw_silent(gs::integers::<u8>().max_value(3));
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || mode == 0 => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    if tc.draw_silent(gs::booleans()) {
                        out.push_str(&format!("\\u{u:04X}"));
                    } else {
                        out.push_str(&format!("\\u{u:04x}"));
                    }
                }
            }
            '/' if mode == 1 => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn render(tc: &TestCase, j: &J, out: &mut String) {
    out.push_str(&ws(tc));
    match j {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::I(i) => out.push_str(&i.to_string()),
        J::U(u) => out.push_str(&u.to_string()),
        J::F(bits) => {
            let f = f64::from_bits(*bits);
            let style = tc.draw_silent(gs::integers::<u8>().max_value(2));
            let s = match style {
                0 => format!("{f:?}"),
                1 => format!("{f:e}"),
                _ => format!("{f:E}"),
            };
            out.push_str(&s);
        }
        J::S(s) => render_str(tc, s, out),
        J::A(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push_str(&ws(tc));
                    out.push(',');
                }
                render(tc, x, out);
                out.push_str(&ws(tc));
            }
            if xs.is_empty() {
                out.push_str(&ws(tc));
            }
            out.push(']');
        }
        J::O(kvs) => {
            out.push('{');
            for (i, (k, v)) in kvs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&ws(tc));
                render_str(tc, k, out);
                out.push_str(&ws(tc));
                out.push(':');
                render(tc, v, out);
                out.push_str(&ws(tc));
            }
            if kvs.is_empty() {
                out.push_str(&ws(tc));
            }
            out.push('}');
        }
    }
    out.push_str(&ws(tc));
}

fn draw_str(tc: &TestCase) -> String {
    tc.draw(gs::one_of(vec![
        gs::text().max_size(80).boxed(),
        gs::text()
            .alphabet("ab\"\\/\n\u{1}\u{7f}é€😀 ")
            .max_size(80)
            .boxed(),
    ]))
}

fn draw_j(tc: &TestCase, depth: usize) -> J {
    let max = if depth >= 4 { 6 } else { 8 };
    match tc.draw(gs::integers::<u8>().max_value(max)) {
        0 => J::Null,
        1 => J::Bool(tc.draw(gs::booleans())),
        2 => J::I(tc.draw(gs::integers::<i64>())),
        3 => J::U(tc.draw(gs::integers::<u64>())),
        4 => J::F(
            tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
                .to_bits(),
        ),
        5 | 6 => J::S(draw_str(tc)),
        7 => {
            let n = tc.draw(gs::integers::<usize>().max_value(6));
            J::A((0..n).map(|_| draw_j(tc, depth + 1)).collect())
        }
        _ => {
            let n = tc.draw(gs::integers::<usize>().max_value(6));
            let mut kvs: Vec<(String, J)> = Vec::new();
            for _ in 0..n {
                let k = draw_str(tc);
                if kvs.iter().all(|(e, _)| *e != k) {
                    kvs.push((k, draw_j(tc, depth + 1)));
                }
            }
            J::O(kvs)
        }
    }
}

fn check_owned(j: &J, v: &OwnedValue) {
    match (j, v) {
        (J::Null, OwnedValue::Static(StaticNode::Null)) => {}
        (J::Bool(a), OwnedValue::Static(StaticNode::Bool(b))) => assert_eq!(a, b),
        (J::I(a), _) => assert_eq!(Some(*a), v.as_i64(), "{v:?}"),
        (J::U(a), _) => assert_eq!(Some(*a), v.as_u64(), "{v:?}"),
        (J::F(bits), OwnedValue::Static(StaticNode::F64(f))) => {
            assert_eq!(f64::from_bits(*bits).to_bits(), f.to_bits(), "{f}")
        }
        (J::F(bits), _) => {
            let f = f64::from_bits(*bits);
            assert!(f.fract() == 0.0, "float {f:?} parsed as {v:?}");
            assert_eq!(v.cast_f64(), Some(f));
        }
        (J::S(a), OwnedValue::String(b)) => assert_eq!(a, b),
        (J::A(a), OwnedValue::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(b.iter()) {
                check_owned(x, y);
            }
        }
        (J::O(a), OwnedValue::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, x) in a {
                check_owned(x, b.get(k.as_str()).unwrap());
            }
        }
        _ => panic!("mismatch {j:?} vs {v:?}"),
    }
}

fn check_borrowed(j: &J, v: &BorrowedValue) {
    let owned: OwnedValue = v.clone().into();
    check_owned(j, &owned);
}

#[hegel::test(test_cases = 2000)]
fn valid_documents_parse_to_model(tc: TestCase) {
    let j = draw_j(&tc, 0);
    let mut text = String::new();
    render(&tc, &j, &mut text);
    tc.note(&format!("input: {text:?}"));

    let mut b1 = text.clone().into_bytes();
    let owned = simd_json::to_owned_value(&mut b1).unwrap();
    check_owned(&j, &owned);

    let mut b2 = text.clone().into_bytes();
    let borrowed = simd_json::to_borrowed_value(&mut b2).unwrap();
    check_borrowed(&j, &borrowed);

    let mut b3 = text.clone().into_bytes();
    let sj: serde_json::Value = simd_json::serde::from_slice(&mut b3).unwrap();
    let expected: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(sj, expected);

    let mut b4 = text.clone().into_bytes();
    let tape = simd_json::to_tape(&mut b4).unwrap();
    let from_tape: OwnedValue = simd_json::value::lazy::Value::from_tape(tape.as_value())
        .into_value()
        .into();
    assert_eq!(from_tape, owned);

    let mut b5 = text.clone().into_bytes();
    let tape = simd_json::to_tape(&mut b5).unwrap();
    let via_tape: serde_json::Value = tape.deserialize().unwrap();
    assert_eq!(via_tape, expected);
}

#[hegel::test(test_cases = 3000)]
fn acceptance_matches_serde_json(tc: TestCase) {
    let j = draw_j(&tc, 0);
    let mut text = String::new();
    render(&tc, &j, &mut text);
    let mut bytes = text.into_bytes();
    let edits = tc.draw(gs::integers::<usize>().max_value(3));
    for _ in 0..edits {
        let kind = tc.draw(gs::integers::<u8>().max_value(2));
        let pos = tc.draw(gs::integers::<usize>().max_value(bytes.len()));
        let byte = tc.draw(gs::sampled_from(
            b"{}[],:\"\\ 0123456789-+.eEtrufalsn\x00\x1f\x7f\xc3\xa9\xff".to_vec(),
        ));
        match kind {
            0 => bytes.insert(pos, byte),
            1 if pos < bytes.len() => {
                bytes.remove(pos);
            }
            _ if pos < bytes.len() => bytes[pos] = byte,
            _ => {}
        }
    }
    tc.note(&format!("input: {:?}", String::from_utf8_lossy(&bytes)));
    let lower = String::from_utf8_lossy(&bytes).to_lowercase();
    tc.assume(
        !["\\ud8", "\\ud9", "\\uda", "\\udb"]
            .iter()
            .any(|p| lower.contains(p)),
    );
    let reference = serde_json::from_slice::<serde_json::Value>(&bytes);
    let mut b1 = bytes.clone();
    let ours = simd_json::to_owned_value(&mut b1);
    match (&reference, &ours) {
        (Ok(r), Ok(o)) => {
            let Ok(o) = TryInto::<serde_json::Value>::try_into(o.clone()) else {
                return;
            };
            assert_eq!(*r, o);
        }
        (Err(_), Err(_)) => {}
        (Err(e), Ok(o)) => {
            if !e.to_string().contains("number out of range") {
                panic!("simd-json accepted invalid JSON (serde_json: {e}): {o:?}")
            }
        }
        (Ok(r), Err(e)) if *e.error() == simd_json::ErrorType::InvalidNumber && has_big_int(r) => {}
        (Ok(r), Err(e)) => panic!("simd-json rejected valid JSON ({e}): {r:?}"),
    }
    let mut b2 = bytes.clone();
    let borrowed = simd_json::to_borrowed_value(&mut b2);
    assert_eq!(borrowed.is_ok(), ours.is_ok());
    let mut b3 = bytes.clone();
    let tape = simd_json::to_tape(&mut b3);
    assert_eq!(tape.is_ok(), ours.is_ok());
}

fn has_big_int(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Number(n) => n
            .as_f64()
            .is_some_and(|f| !n.is_i64() && !n.is_u64() && f.abs() >= 9.2e18),
        serde_json::Value::Array(xs) => xs.iter().any(has_big_int),
        serde_json::Value::Object(m) => m.values().any(has_big_int),
        _ => false,
    }
}

#[hegel::test(test_cases = 2000)]
fn value_encoders_round_trip(tc: TestCase) {
    let j = draw_j(&tc, 0);
    let mut text = String::new();
    render(&tc, &j, &mut text);
    let mut b = text.clone().into_bytes();
    let owned = simd_json::to_owned_value(&mut b).unwrap();
    let mut b = text.clone().into_bytes();
    let borrowed = simd_json::to_borrowed_value(&mut b).unwrap();
    let expected: serde_json::Value = serde_json::from_str(&text).unwrap();
    let encodings = [
        ("owned.encode", owned.encode()),
        ("owned.encode_pretty", owned.encode_pp()),
        ("borrowed.encode", borrowed.encode()),
        ("borrowed.encode_pretty", borrowed.encode_pp()),
        ("to_string(owned)", simd_json::to_string(&owned).unwrap()),
        (
            "to_string_pretty(owned)",
            simd_json::to_string_pretty(&owned).unwrap(),
        ),
        (
            "to_string(borrowed)",
            simd_json::to_string(&borrowed).unwrap(),
        ),
        (
            "serde_json::to_string(owned)",
            serde_json::to_string(&owned).unwrap(),
        ),
    ];
    for (name, enc) in encodings {
        let back: serde_json::Value = serde_json::from_str(&enc)
            .unwrap_or_else(|e| panic!("{name} produced invalid JSON ({e}): {enc}"));
        assert_eq!(back, expected, "{name}: {enc}");
        let mut eb = enc.clone().into_bytes();
        let again = simd_json::to_owned_value(&mut eb).unwrap();
        assert_eq!(again, owned, "{name}: {enc}");
    }
}
