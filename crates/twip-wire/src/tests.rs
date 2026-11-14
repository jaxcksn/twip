use super::*;

#[test]
fn strings_round_trip_with_serde_json_escapes() {
    for s in ["", "plain", "quote \" back \\ slash", "tab\tnl\ncr\r", "\u{1}\u{1f}\u{8}\u{c}", "é 中 😀", "\u{7f}"] {
        let out = to_vec(s);
        assert_eq!(String::from_utf8(out.clone()).unwrap(), json_escape(s));
        assert_eq!(from_slice::<String>(&out).unwrap(), s);
    }
    assert_eq!(from_str::<String>(r#""é😀\/""#).unwrap(), "é😀/");
    for bad in [r#""\ud83d""#, r#""\ude00""#, r#""\u+0041""#, r#""\x""#, "\"a\u{1}\"", r#""open"#] {
        assert!(from_str::<String>(bad).is_err(), "{bad}");
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out + "\""
}

#[test]
fn numbers_follow_the_json_grammar() {
    assert_eq!(from_str::<u32>("42").unwrap(), 42);
    assert_eq!(from_str::<i64>("-7").unwrap(), -7);
    assert_eq!(from_str::<f64>("1.5e3").unwrap(), 1500.0);
    assert_eq!(from_str::<f64>("2").unwrap(), 2.0);
    for bad in ["01", "1.", ".5", "-", "1e", "+1", "1.5", "256"] {
        assert!(from_str::<u8>(bad).is_err(), "{bad}");
    }
    assert_eq!(String::from_utf8(to_vec(&1.0f64)).unwrap(), "1.0");
    assert_eq!(String::from_utf8(to_vec(&f64::NAN)).unwrap(), "null");
}

#[test]
fn nesting_is_bounded() {
    let deep = "[".repeat(MAX_DEPTH as usize + 1) + &"]".repeat(MAX_DEPTH as usize + 1);
    assert!(Reader::new(deep.as_bytes()).unwrap().skip().unwrap_err().message().contains("recursion"));
    let ok = "[".repeat(MAX_DEPTH as usize) + &"]".repeat(MAX_DEPTH as usize);
    Reader::new(ok.as_bytes()).unwrap().skip().unwrap();
}

#[test]
fn objects_and_arrays_need_their_separators() {
    for bad in [r#"{"a":1 "b":2}"#, r#"{,"a":1}"#, r#"{"a":1,}"#, "[1 2]", "[,1]", "[1,]", r#"{"a" 1}"#] {
        assert!(Reader::new(bad.as_bytes()).unwrap().skip().is_err(), "{bad}");
    }
    assert_eq!(from_str::<Vec<(u8, String)>>(r#" [ [1, "a"] , [2,"b"] ] "#).unwrap(), vec![(1, "a".into()), (2, "b".into())]);
    assert!(from_str::<u8>("1 2").is_err());
}

#[test]
fn tag_scans_ahead_without_moving() {
    let json = br#"{"a":{"type":"inner"},"b":[1,{"x":2}],"type":"outer"}"#;
    let r = Reader::new(json).unwrap();
    assert_eq!(r.tag("type").unwrap(), "outer");
    assert_eq!(r.pos, 0);
    assert!(r.tag("kind").is_err());
}
