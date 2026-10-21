#![cfg(all(
    not(target_family = "wasm"),
    feature = "serde_impl",
    not(feature = "approx-number-parsing")
))]

use hegel::TestCase;
use hegel::generators as gs;
use simd_json::prelude::*;
use simd_json::{ErrorType, OwnedValue, StaticNode};

fn digits(tc: &TestCase, min: usize, max: usize) -> String {
    tc.draw(
        gs::text()
            .alphabet("0123456789")
            .min_size(min)
            .max_size(max),
    )
}

fn draw_number(tc: &TestCase) -> String {
    let mut s = String::new();
    if tc.draw(gs::booleans()) {
        s.push('-');
    }
    let int = digits(tc, 1, 30);
    let int = int.trim_start_matches('0');
    s.push_str(if int.is_empty() { "0" } else { int });
    if tc.draw(gs::booleans()) {
        s.push('.');
        s.push_str(&digits(tc, 1, 40));
    }
    if tc.draw(gs::booleans()) {
        s.push(tc.draw(gs::sampled_from(vec!['e', 'E'])));
        s.push_str(tc.draw(gs::sampled_from(vec!["", "+", "-"])));
        s.push_str(&digits(tc, 1, 4));
    }
    s
}

enum Expect {
    Int(i128),
    Float(f64),
    Error,
}

fn expected(lit: &str) -> Expect {
    let is_int = !lit.contains(['.', 'e', 'E']);
    if is_int {
        let v: i128 = match lit.parse() {
            Ok(v) => v,
            Err(_) => return Expect::Error,
        };
        if v >= i128::from(i64::MIN) && v <= i128::from(u64::MAX) {
            return Expect::Int(v);
        }
        return Expect::Error;
    }
    let f: f64 = lit.parse().unwrap();
    if f.is_finite() {
        Expect::Float(f)
    } else {
        Expect::Error
    }
}

fn check(lit: &str, got: Result<OwnedValue, simd_json::Error>) {
    match (expected(lit), got) {
        (Expect::Int(i), Ok(v)) => {
            let g = v
                .as_i64()
                .map(i128::from)
                .or_else(|| v.as_u64().map(i128::from));
            assert_eq!(g, Some(i), "{lit} -> {v:?}");
        }
        (Expect::Float(f), Ok(OwnedValue::Static(StaticNode::F64(g)))) => {
            assert_eq!(f.to_bits(), g.to_bits(), "{lit}: expected {f:e}, got {g:e}");
        }
        (Expect::Error, Err(e)) => assert_eq!(*e.error(), ErrorType::InvalidNumber, "{lit}"),
        (Expect::Int(i), Err(e)) => panic!("{lit}: expected int {i}, got error {e}"),
        (Expect::Float(f), Err(e)) => panic!("{lit}: expected {f:e}, got error {e}"),
        (Expect::Float(f), Ok(v)) => panic!("{lit}: expected float {f:e}, got {v:?}"),
        (Expect::Error, Ok(v)) => panic!("{lit}: expected error, got {v:?}"),
    }
}

#[cfg(not(any(
    feature = "approx-number-parsing",
    feature = "big-int-as-float",
    feature = "128bit"
)))]
#[hegel::test(test_cases = 5000)]
fn number_literals_parse_exactly(tc: TestCase) {
    let lit = draw_number(&tc);
    let pad = tc.draw(gs::integers::<usize>().max_value(130));
    let in_array = tc.draw(gs::booleans());
    let doc = if in_array {
        format!("{}[{lit}]", " ".repeat(pad))
    } else {
        format!("{}{lit}", " ".repeat(pad))
    };
    tc.note(&format!("doc: {doc:?}"));
    let mut b = doc.clone().into_bytes();
    let got = simd_json::to_owned_value(&mut b).map(|v| {
        if in_array {
            v.as_array().unwrap()[0].clone()
        } else {
            v
        }
    });
    check(&lit, got);

    let mut b = doc.into_bytes();
    let tape = simd_json::to_tape(&mut b);
    assert_eq!(tape.is_ok(), !matches!(expected(&lit), Expect::Error));
}

fn escape(tc: &TestCase, s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let esc = tc.draw_silent(gs::integers::<u8>().max_value(4)) == 0;
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || esc => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{u:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

#[hegel::test(test_cases = 5000)]
fn strings_round_trip_through_every_api(tc: TestCase) {
    let s = tc.draw(gs::one_of(vec![
        hegel::Generator::boxed(gs::text().max_size(300)),
        hegel::Generator::boxed(
            gs::text()
                .alphabet("a\"\\/\u{0}\u{1f}é😀\u{2028}")
                .max_size(300),
        ),
    ]));
    let pad = tc.draw(gs::integers::<usize>().max_value(130));
    let key_too = tc.draw(gs::booleans());
    let lit = escape(&tc, &s);
    let doc = if key_too {
        format!("{}{{\"{lit}\":\"{lit}\"}}", " ".repeat(pad))
    } else {
        format!("{}\"{lit}\"", " ".repeat(pad))
    };
    tc.note(&format!("doc: {doc:?}"));
    let mut b = doc.clone().into_bytes();
    let v = simd_json::to_owned_value(&mut b).unwrap();
    let got = if key_too {
        let o = v.as_object().unwrap();
        assert_eq!(o.len(), 1);
        let (k, val) = o.iter().next().unwrap();
        assert_eq!(k, &s);
        val.as_str().unwrap().to_string()
    } else {
        v.as_str().unwrap().to_string()
    };
    assert_eq!(got, s);

    let mut b = doc.clone().into_bytes();
    let bv = simd_json::to_borrowed_value(&mut b).unwrap();
    assert_eq!(OwnedValue::from(bv), v);

    let mut b = doc.clone().into_bytes();
    let sv: serde_json::Value = simd_json::serde::from_slice(&mut b).unwrap();
    assert_eq!(sv, serde_json::from_str::<serde_json::Value>(&doc).unwrap());

    if !key_too {
        let mut b = doc.clone().into_bytes();
        let st: String = simd_json::serde::from_slice(&mut b).unwrap();
        assert_eq!(st, s);
        let mut b = doc.clone().into_bytes();
        let st: &str = match simd_json::serde::from_slice::<&str>(&mut b) {
            Ok(x) => x,
            Err(_) => return,
        };
        assert_eq!(st, s);
    }
}

#[hegel::test(test_cases = 3000)]
fn buffers_reuse_matches_fresh_parse(tc: TestCase) {
    let docs: Vec<String> = tc.draw(gs::vecs(gs::one_of(vec![
        hegel::Generator::boxed(gs::sampled_from(vec![
            "[1,2,3]".to_string(),
            "{\"a\":[true,false,null],\"b\":\"x\\ny\"}".to_string(),
            "\"".to_string(),
            "[[[[[[[[[[".to_string(),
            "{\"k\":".to_string(),
            "1.5e3".to_string(),
            "   ".to_string(),
            "".to_string(),
            "[\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\u00e9\"]".to_string(),
        ])),
        hegel::Generator::boxed(gs::text().alphabet("[]{}\":,1 atrue\\").max_size(200)),
    ])).max_size(8));
    let cap = tc.draw(gs::integers::<usize>().max_value(300));
    let mut buffers = simd_json::Buffers::new(cap);
    for d in &docs {
        let mut fresh = d.clone().into_bytes();
        let expected = simd_json::to_owned_value(&mut fresh);
        let mut reused = d.clone().into_bytes();
        let got = simd_json::owned::to_value_with_buffers(&mut reused, &mut buffers);
        match (&expected, &got) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{d:?}"),
            (Err(_), Err(_)) => {}
            _ => panic!("{d:?}: fresh {expected:?} vs reused {got:?}"),
        }
        let mut r2 = d.clone().into_bytes();
        let got2 =
            simd_json::borrowed::to_value_with_buffers(&mut r2, &mut buffers).map(OwnedValue::from);
        assert_eq!(got2.is_ok(), expected.is_ok(), "{d:?}");
        if let (Ok(a), Ok(b)) = (&expected, &got2) {
            assert_eq!(a, b);
        }
    }
}
