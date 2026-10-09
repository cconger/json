#![allow(clippy::assertions_on_result_states)]

use serde::{Deserialize, Serialize};
#[cfg(feature = "raw_value")]
use serde_json::value::RawValue;
#[cfg(feature = "arbitrary_precision")]
use serde_json::Number;
use serde_json::{Map, Value};
#[cfg(feature = "std")]
use std::io::Cursor;

const RAW: &str = "$serde_json::private::RawValue";
const NUM: &str = "$serde_json::private::Number";

// Build the expectation independently of the conversion under test.
fn object(key: &str, value: Value) -> Value {
    let mut map = Map::new();
    map.insert(key.to_owned(), value);
    Value::Object(map)
}

fn text(s: &str) -> Value {
    Value::String(s.to_owned())
}

fn assert_all_paths(value: &Value) {
    let json = serde_json::to_string(value).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), *value);
    assert_eq!(
        serde_json::from_slice::<Value>(json.as_bytes()).unwrap(),
        *value
    );
    #[cfg(feature = "std")]
    assert_eq!(
        serde_json::from_reader::<_, Value>(Cursor::new(&json)).unwrap(),
        *value
    );
    assert_eq!(
        serde_json::from_value::<Value>(value.clone()).unwrap(),
        *value
    );
    assert_eq!(Value::deserialize(value).unwrap(), *value);
    assert_eq!(serde_json::to_value(value).unwrap(), *value);
}

#[test]
fn object_keys_remain_literal() {
    for key in [RAW, NUM] {
        for value in [
            text("null"),
            text("123"),
            text("1.25"),
            text("[[[[0]]]]"),
            text("not json or a number"),
            Value::Null,
            Value::from(42),
            Value::Array(vec![]),
            object(RAW, text("null")),
        ] {
            let obj = object(key, value.clone());
            assert_all_paths(&obj);
            assert_all_paths(&Value::Array(vec![Value::Null, object("nested", obj)]));

            let mut first = Map::new();
            first.insert(key.to_owned(), value.clone());
            first.insert("second".into(), Value::Bool(true));
            assert_all_paths(&Value::Object(first));

            let mut last = Map::new();
            last.insert("first".into(), Value::Bool(false));
            last.insert(key.to_owned(), value.clone());
            assert_all_paths(&Value::Object(last));

            assert_all_paths(&object(&format!("prefix{key}"), value));
        }
    }
}

#[test]
fn escaped_object_keys_remain_literal() {
    for key in [RAW, NUM] {
        let expected = object(key, text("123"));
        for (idx, ch) in key.char_indices() {
            let escaped = format!(
                "{}\\u{:04x}{}",
                &key[..idx],
                u32::from(ch),
                &key[idx + ch.len_utf8()..]
            );
            let json = format!(r#"{{"{escaped}":"123"}}"#);
            assert_eq!(
                serde_json::from_str::<Value>(&json).unwrap(),
                expected,
                "{json}"
            );
        }
    }
}

#[test]
fn string_contents_never_restart_the_value_parser() {
    let mut payload = "0".to_owned();
    for _ in 0..5 {
        payload = format!("{}{}{}", "[".repeat(80), payload, "]".repeat(80));
        payload = serde_json::to_string(&object(RAW, text(&payload))).unwrap();
    }
    assert_all_paths(&object(RAW, text(&payload)));

    let within_limit = format!("{}0{}", "[".repeat(127), "]".repeat(127));
    let too_deep = format!("{}0{}", "[".repeat(128), "]".repeat(128));
    assert!(serde_json::from_str::<Value>(&within_limit).is_ok());
    assert!(serde_json::from_str::<Value>(&too_deep)
        .unwrap_err()
        .to_string()
        .contains("recursion limit exceeded"));
}

#[cfg(feature = "arbitrary_precision")]
#[test]
fn genuine_numbers_preserve_precision_on_all_paths() {
    for json in [
        "0",
        "-42",
        "1.25",
        "1e100",
        "1e-999",
        "1e400",
        "12345678901234567890123456789012345678901234567890",
        "0.123456789012345678901234567890",
        "1.234e+45",
    ] {
        let number: Number = serde_json::from_str(json).unwrap();
        let value: Value = serde_json::from_str(json).unwrap();
        assert_eq!(value, Value::Number(number.clone()), "{json}");
        assert_eq!(
            serde_json::from_value::<Number>(value.clone()).unwrap(),
            number,
            "{json}"
        );
        assert_all_paths(&value);
        assert_all_paths(&Value::Array(vec![value.clone(), object("n", value)]));
    }
}

#[cfg(feature = "arbitrary_precision")]
#[test]
fn number_requires_a_number_not_a_similarly_named_object() {
    let value = object(NUM, text("1.25"));
    let json = serde_json::to_string(&value).unwrap();
    assert!(serde_json::from_str::<Number>(&json).is_err());
    assert!(serde_json::from_value::<Number>(value).is_err());
}

#[cfg(feature = "raw_value")]
#[test]
fn explicit_raw_values_still_work_including_literal_private_keys() {
    #[derive(Serialize, Deserialize)]
    struct Wrapped {
        body: Box<RawValue>,
    }

    for json in [
        r#" { "some" :  0 } "#,
        "1.234",
        r#"{"$serde_json::private::RawValue":"null"}"#,
        r#"{"$serde_json::private::Number":"hello"}"#,
    ] {
        let raw: Box<RawValue> = serde_json::from_str(json).unwrap();
        assert_eq!(raw.get(), json.trim());
        let value: Value = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_value(&raw).unwrap(), value);
        let round_raw: Box<RawValue> = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(round_raw.get()).unwrap(),
            value
        );
        let wrapper = Wrapped { body: raw };
        let round: Wrapped =
            serde_json::from_str(&serde_json::to_string(&wrapper).unwrap()).unwrap();
        assert_eq!(wrapper.body.get(), round.body.get());
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Tagged {
    Input { value: Value },
    Output { value: Value },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    ValidObject { keep: bool },
    Fallback(Value),
}

// Serde buffers internally tagged and untagged content before Value sees it.
// Check that buffering does not turn strings into private keys or lose the
// distinction between genuine arbitrary-precision numbers and objects.
#[test]
fn buffered_values_preserve_key_provenance() {
    let mut values = vec![
        "1.25",
        r#"{"$serde_json::private::RawValue":"null"}"#,
        r#"{"$serde_json::private::Number":"123"}"#,
    ];
    if cfg!(feature = "arbitrary_precision") {
        values.extend([
            "1e400",
            "1e-999",
            "12345678901234567890123456789012345678901234567890",
        ]);
    }
    for json in values {
        let value: Value = serde_json::from_str(json).unwrap();
        for expected in [
            Tagged::Input {
                value: value.clone(),
            },
            Tagged::Output {
                value: value.clone(),
            },
        ] {
            let encoded = serde_json::to_vec(&expected).unwrap();
            assert_eq!(
                serde_json::from_slice::<Tagged>(&encoded).unwrap(),
                expected,
                "{json}"
            );
        }
        let expected = Untagged::Fallback(value);
        let encoded = serde_json::to_string(&expected).unwrap();
        assert_eq!(
            serde_json::from_str::<Untagged>(&encoded).unwrap(),
            expected,
            "{json}"
        );
        let encoded = serde_json::to_value(&expected).unwrap();
        assert_eq!(
            serde_json::from_value::<Untagged>(encoded).unwrap(),
            expected,
            "{json}"
        );
    }
}
