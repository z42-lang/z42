use super::*;
use crate::metadata::Value;

// __str_to_chars: bulk materialise the whole char[] in one native call — the
// array-view primitive backing script-side string ops (String.ToCharArray →
// IndexOf/…). Must yield Unicode scalars (chars), not bytes.
#[test]
fn to_chars_yields_scalars() {
    let ctx = VmContext::new();
    let out = builtin_str_to_chars(&ctx, &[Value::Str("héllo".into())]).unwrap();
    match out {
        Value::Array(a) => {
            let got: Vec<char> = a.borrow().iter().map(|v| match v {
                Value::Char(c) => *c,
                other => panic!("expected char, got {:?}", other),
            }).collect();
            // "héllo" = 5 scalars (é is one scalar though 2 UTF-8 bytes).
            assert_eq!(got, vec!['h', 'é', 'l', 'l', 'o']);
        }
        other => panic!("expected Array, got {:?}", other),
    }
}

#[test]
fn to_chars_empty() {
    let ctx = VmContext::new();
    let out = builtin_str_to_chars(&ctx, &[Value::Str("".into())]).unwrap();
    match out {
        Value::Array(a) => assert_eq!(a.borrow().len(), 0),
        other => panic!("expected empty Array, got {:?}", other),
    }
}
