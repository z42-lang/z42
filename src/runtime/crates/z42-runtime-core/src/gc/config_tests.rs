use super::*;

#[test]
fn promotion_line_clamps_to_the_header_age_range() {
    for (input, expected) in [
        (None, PROMOTION_THRESHOLD),
        (Some(0), 1),
        (Some(1), 1),
        (Some(u8::MAX), crate::object::header::MAX_GEN_AGE),
    ] {
        let config = GcConfig {
            promotion_age: input,
            ..GcConfig::default()
        };
        assert_eq!(config.promotion_age(), expected);
    }
}
