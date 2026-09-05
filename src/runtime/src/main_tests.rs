use super::{libs_env_to_publish, Cli};
use std::path::Path;

// VM resolved a dir and Z42_LIBS is unset → publish the resolved dir so the
// in-process z42c sees the same libs dir (SDK layout works with no env set).
#[test]
fn unset_env_publishes_resolved_dir() {
    let got = libs_env_to_publish(None, Some(Path::new("/sdk/libs")));
    assert_eq!(got.as_deref(), Some("/sdk/libs"));
}

// Empty string counts as unset (mirrors RuntimeConfig env handling).
#[test]
fn empty_env_is_treated_as_unset() {
    let got = libs_env_to_publish(Some("  "), Some(Path::new("/sdk/libs")));
    assert_eq!(got.as_deref(), Some("/sdk/libs"));
}

// Explicit Z42_LIBS is the caller's deliberate choice → never overridden.
#[test]
fn explicit_env_is_left_untouched() {
    assert_eq!(libs_env_to_publish(Some("/my/libs"), Some(Path::new("/sdk/libs"))), None);
}

// Nothing resolved anywhere → nothing to publish (z42c keeps its no-deps
// degraded path, unchanged from before).
#[test]
fn no_resolution_publishes_nothing() {
    assert_eq!(libs_env_to_publish(None, None), None);
}

// ── tidy-z42vm-cli: 帮助文本是用户文档 ──────────────────────────────────────

/// `--help` 里不该出现内部溯源信息。曾经的样子：
///   `--info    ... Useful for bug reports and CI preflight. docs/review.md Part 4 D5 (2026-05-25)`
///   `--strict-config  ... complete-runtime-settings P1 (2026-09-05) — the CI config-drift gate`
/// 那是给维护者的，对着 `--help` 的人不需要，还挤掉了真正该说的内容。溯源移进了
/// 紧邻的 `//` 注释（信息一条不丢，只是换了读者）——这条门守住不再漂回去。
#[test]
fn help_text_carries_no_internal_provenance() {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let help = cmd.render_long_help().to_string();

    for needle in ["docs/review.md", "docs/spec", "complete-runtime-settings",
                   "script-profiling", "add-z42-launcher", "retire-z-codes",
                   "2026-05-", "2026-06-", "2026-09-"] {
        assert!(!help.contains(needle),
            "`--help` 泄漏了内部溯源信息 {needle:?} —— 它属于代码注释，不属于用户帮助");
    }
}

#[test]
fn help_text_groups_options_by_responsibility() {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let help = cmd.render_long_help().to_string();
    for heading in ["执行:", "运行时配置:", "自省:", "诊断:"] {
        assert!(help.contains(heading), "`--help` 缺少分组 {heading:?}");
    }
}

#[test]
fn the_three_introspection_commands_are_distinguishable() {
    // 从前三条帮助都以 "Print ..." 开头，看不出区别。
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let help = cmd.render_long_help().to_string();
    assert!(help.contains("提 bug 时贴这个"), "--info 该说清它的用途");
    assert!(help.contains("有哪些旋钮"), "--list-knobs 该说清它列的是 schema");
    assert!(help.contains("当前是什么值"), "--show-config 该说清它列的是生效值");
}
