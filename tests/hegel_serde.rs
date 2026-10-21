#![cfg(all(
    not(target_family = "wasm"),
    feature = "serde_impl",
    not(feature = "approx-number-parsing")
))]

use std::collections::BTreeMap;

use hegel::generators as gs;
use hegel::{Generator, TestCase};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Unit;
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct New(u32);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Tup(i8, String);

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
enum E {
    A,
    B(i64),
    C(u8, bool),
    D { x: f64, y: Option<String> },
    V(Vec<u16>),
    S {},
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "type")]
enum Internal {
    One { a: u8 },
    Two { b: String, c: Option<bool> },
    Three,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(tag = "t", content = "c")]
enum Adjacent {
    X(i32),
    Y { s: String },
    Z,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
enum Untagged {
    Num(i64),
    Text(String),
    List(Vec<bool>),
    Obj { k: u8 },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct Inner {
    p: u16,
    q: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct S {
    a: i8,
    b: i16,
    c: i32,
    d: i64,
    e: u8,
    f: u16,
    g: u32,
    h: u64,
    fl32: f32,
    fl64: f64,
    bo: bool,
    ch: char,
    s: String,
    o: Option<i32>,
    v: Vec<E>,
    m: BTreeMap<String, E>,
    t: (u8, String, Unit),
    arr: [i16; 3],
    unit: (),
    us: Unit,
    n: New,
    tu: Tup,
    en: E,
    internal: Internal,
    adjacent: Adjacent,
    untagged: Untagged,
    #[serde(flatten)]
    inner: Inner,
}

fn s(tc: &TestCase) -> String {
    tc.draw(gs::one_of(vec![
        gs::text().max_size(20).boxed(),
        gs::text().alphabet("ab\"\\\u{0}é😀").max_size(20).boxed(),
    ]))
}

fn f64v(tc: &TestCase) -> f64 {
    tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(false))
}

fn e(tc: &TestCase, depth: usize) -> E {
    match tc.draw(gs::integers::<u8>().max_value(6)) {
        0 => E::A,
        1 => E::B(tc.draw(gs::integers())),
        2 => E::C(tc.draw(gs::integers()), tc.draw(gs::booleans())),
        3 => E::D {
            x: f64v(tc),
            y: tc.draw(gs::optional(gs::text().max_size(5))),
        },
        4 => E::V(tc.draw(gs::vecs(gs::integers()).max_size(3))),
        5 => E::A,
        _ if depth > 0 => E::A,
        _ => E::S {},
    }
}

fn draw_s(tc: &TestCase) -> S {
    S {
        a: tc.draw(gs::integers()),
        b: tc.draw(gs::integers()),
        c: tc.draw(gs::integers()),
        d: tc.draw(gs::integers()),
        e: tc.draw(gs::integers()),
        f: tc.draw(gs::integers()),
        g: tc.draw(gs::integers()),
        h: tc.draw(gs::integers()),
        fl32: tc.draw(gs::floats::<f32>().allow_nan(false).allow_infinity(false)),
        fl64: f64v(tc),
        bo: tc.draw(gs::booleans()),
        ch: tc.draw(gs::characters()),
        s: s(tc),
        o: tc.draw(gs::optional(gs::integers())),
        v: (0..tc.draw(gs::integers::<usize>().max_value(3)))
            .map(|_| e(tc, 1))
            .collect(),
        m: (0..tc.draw(gs::integers::<usize>().max_value(3)))
            .map(|_| (s(tc), e(tc, 1)))
            .collect(),
        t: (tc.draw(gs::integers()), s(tc), Unit),
        arr: [
            tc.draw(gs::integers()),
            tc.draw(gs::integers()),
            tc.draw(gs::integers()),
        ],
        unit: (),
        us: Unit,
        n: New(tc.draw(gs::integers())),
        tu: Tup(tc.draw(gs::integers()), s(tc)),
        en: e(tc, 0),
        internal: match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 => Internal::One {
                a: tc.draw(gs::integers()),
            },
            1 => Internal::Two {
                b: s(tc),
                c: tc.draw(gs::optional(gs::booleans())),
            },
            _ => Internal::Three,
        },
        adjacent: match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 => Adjacent::X(tc.draw(gs::integers())),
            1 => Adjacent::Y { s: s(tc) },
            _ => Adjacent::Z,
        },
        untagged: match tc.draw(gs::integers::<u8>().max_value(3)) {
            0 => Untagged::Num(tc.draw(gs::integers())),
            1 => Untagged::Text(s(tc)),
            2 => Untagged::List(tc.draw(gs::vecs(gs::booleans()).max_size(3))),
            _ => Untagged::Obj {
                k: tc.draw(gs::integers()),
            },
        },
        inner: Inner {
            p: tc.draw(gs::integers()),
            q: tc.draw(gs::optional(gs::text().max_size(5))),
        },
    }
}

fn as_json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("invalid JSON ({e}): {text}"))
}

#[hegel::test(test_cases = 2000)]
fn serializers_agree_with_serde_json(tc: TestCase) {
    let v = draw_s(&tc);
    let reference = serde_json::to_value(&v).unwrap();
    assert_eq!(as_json(&simd_json::to_string(&v).unwrap()), reference);
    assert_eq!(
        as_json(&simd_json::to_string_pretty(&v).unwrap()),
        reference
    );
    let bytes = simd_json::to_vec(&v).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        reference
    );
    let ov = simd_json::serde::to_owned_value(&v).unwrap();
    assert_eq!(serde_json::to_value(&ov).unwrap(), reference);
    let bv = simd_json::serde::to_borrowed_value(&v).unwrap();
    assert_eq!(
        serde_json::to_value(simd_json::OwnedValue::from(bv)).unwrap(),
        reference
    );
}

#[hegel::test(test_cases = 2000)]
fn deserializers_round_trip(tc: TestCase) {
    let v = draw_s(&tc);
    let text = serde_json::to_string(&v).unwrap();
    tc.note(&format!("json: {text}"));

    let mut b = text.clone().into_bytes();
    let got: S = simd_json::from_slice(&mut b).unwrap();
    assert_eq!(got, v, "from_slice");

    let got: S = simd_json::from_reader(text.as_bytes()).unwrap();
    assert_eq!(got, v, "from_reader");

    let mut b = text.clone().into_bytes();
    let ov = simd_json::to_owned_value(&mut b).unwrap();
    let got: S = simd_json::serde::from_refowned_value(&ov).unwrap();
    assert_eq!(got, v, "from_refowned_value");
    let got: S = simd_json::serde::from_owned_value(ov).unwrap();
    assert_eq!(got, v, "from_owned_value");

    let mut b = text.clone().into_bytes();
    let bv = simd_json::to_borrowed_value(&mut b).unwrap();
    let got: S = simd_json::serde::from_refborrowed_value(&bv).unwrap();
    assert_eq!(got, v, "from_refborrowed_value");
    let got: S = simd_json::serde::from_borrowed_value(bv).unwrap();
    assert_eq!(got, v, "from_borrowed_value");

    let mut b = text.clone().into_bytes();
    let tape = simd_json::to_tape(&mut b).unwrap();
    let got: S = tape.deserialize().unwrap();
    assert_eq!(got, v, "tape");

    let ov = simd_json::serde::to_owned_value(&v).unwrap();
    let got: S = simd_json::serde::from_owned_value(ov).unwrap();
    assert_eq!(got, v, "to_owned_value -> from_owned_value");
    let pretty = simd_json::to_string_pretty(&v).unwrap();
    let mut b = pretty.into_bytes();
    let got: S = simd_json::from_slice(&mut b).unwrap();
    assert_eq!(got, v, "pretty round trip");
}

#[hegel::test(test_cases = 3000)]
fn typed_acceptance_matches_serde_json(tc: TestCase) {
    let v = draw_s(&tc);
    let text = serde_json::to_string(&v).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(&text).unwrap();
    let obj = j.as_object_mut().unwrap();
    let keys: Vec<String> = obj.keys().cloned().collect();
    let k = tc.draw(gs::sampled_from(keys));
    let replacement = tc.draw(gs::sampled_from(vec![
        serde_json::json!(null),
        serde_json::json!(true),
        serde_json::json!(-1),
        serde_json::json!(256),
        serde_json::json!(70000),
        serde_json::json!(4294967296u64),
        serde_json::json!(1.5),
        serde_json::json!(1.0),
        serde_json::json!(""),
        serde_json::json!("ab"),
        serde_json::json!("A"),
        serde_json::json!([]),
        serde_json::json!([1]),
        serde_json::json!([1, 2, 3, 4]),
        serde_json::json!({}),
        serde_json::json!({"A": null}),
        serde_json::json!({"B": 1, "C": [1, true]}),
        serde_json::json!({"type": "Three"}),
        serde_json::json!({"t": "Z"}),
        serde_json::json!({"t": "X", "c": 1, "extra": 2}),
    ]));
    if tc.draw(gs::booleans()) {
        obj.insert(k, replacement);
    } else {
        obj.remove(&k);
    }
    let text = serde_json::to_string(&j).unwrap();
    tc.note(&format!("json: {text}"));
    let reference = serde_json::from_str::<S>(&text);
    let mut b = text.clone().into_bytes();
    let ours = simd_json::from_slice::<S>(&mut b);
    let mut b = text.clone().into_bytes();
    let ov = simd_json::to_owned_value(&mut b).unwrap();
    let via_value = simd_json::serde::from_refowned_value::<S>(&ov);
    match (&reference, &ours) {
        (Ok(r), Ok(o)) => assert_eq!(r, o),
        (Err(_), Err(_)) => {}
        _ => panic!("serde_json: {reference:?}\nsimd_json::from_slice: {ours:?}"),
    }
    match (&reference, &via_value) {
        (Ok(r), Ok(o)) => assert_eq!(r, o),
        (Err(_), Err(_)) => {}
        _ => panic!("serde_json: {reference:?}\nfrom_refowned_value: {via_value:?}"),
    }
}
