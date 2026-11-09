use super::*;
#[test]
fn jit_mode_request_survives_parsing_independently_of_static_linkage() {
    let parsed = Cli::try_parse_from(["z42vm", "app.zpkg", "--mode", "jit"]).unwrap();
    assert!(matches!(parsed.mode, Some(ExecMode::Jit)));
    assert_eq!(resolve_config_mode(Some("jit")), Some(z42::host::ExecMode::Jit));
}
