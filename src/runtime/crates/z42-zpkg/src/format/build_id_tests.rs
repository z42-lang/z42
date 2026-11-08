use super::*;

#[test]
fn short_hex_lowercase_first_4_bytes() {
    let id = [0xAB, 0xCD, 0x12, 0x34, 0xFF, 0xFF];
    assert_eq!(short_hex(&id), "abcd1234");
}
