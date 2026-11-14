//! This integration harness has its own process-wide configuration cell.
#[test]
fn availability_queries_do_not_publish_configuration_before_the_host() {
    use z42::{config::{init_runtime_config, runtime_config, RuntimeConfig}, host::{execution_available, ExecMode}};
    let _ = execution_available(ExecMode::Interp);
    let _ = execution_available(ExecMode::Jit);
    assert!(!execution_available(ExecMode::Aot));

    let mut requested = RuntimeConfig::default();
    requested.native_search_paths = vec!["host-selected-native-directory".into()];
    assert!(init_runtime_config(requested).is_ok(), "a read-only capability query installed defaults before the host");
    assert_eq!(runtime_config().native_search_paths, [std::path::PathBuf::from("host-selected-native-directory")]);
}
