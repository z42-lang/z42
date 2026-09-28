//! `expand_probing_paths` 的规则（add-deployment-model 批 3）。
//!
//! 守的是 design.md §probing-paths 的解析规则那张表——每条规则一个测试，而不是把它们
//! 混在一个"大概能用"的用例里：这些规则彼此独立，混在一起测就分不清是哪条坏了。

use crate::probing::{expand_probing_paths, expand_probing_paths_with};
use std::path::PathBuf;

/// 在临时目录里搭一棵树，返回根。
fn tree(name: &str, dirs: &[&str]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("z42-probing-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for d in dirs {
        std::fs::create_dir_all(root.join(d)).unwrap();
    }
    root
}

#[test]
fn relative_resolves_against_entry_dir_not_cwd() {
    // 这条是整个设计的要害：同一个安装从任何工作目录启动都必须一样。
    let root = tree("rel", &["app", "shared"]);
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../shared")]);
    assert_eq!(got, vec![root.join("shared")], "相对项须相对 entry-dir 解析");
}

#[test]
fn absolute_is_used_as_is() {
    let root = tree("abs", &["elsewhere"]);
    let entry = root.join("elsewhere");
    let abs = root.join("elsewhere");
    let got = expand_probing_paths(&entry, &[abs.clone()]);
    assert_eq!(got, vec![abs]);
}

#[test]
fn missing_dir_is_skipped_not_an_error() {
    // 可选的插件目录不该让启动失败。
    let root = tree("missing", &["app"]);
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../nope")]);
    assert!(got.is_empty(), "不存在的项应被静默跳过，实得 {got:?}");
}

#[test]
fn single_star_matches_one_level_sorted() {
    let root = tree("star", &["app", "plugins/b", "plugins/a", "plugins/c"]);
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../plugins/*")]);
    let want: Vec<PathBuf> = ["a", "b", "c"].iter().map(|n| root.join("plugins").join(n)).collect();
    // 顺序必须稳定：同名 zpkg 落在两个目录时，解析结果不能靠 readdir 的运气。
    assert_eq!(got, want, "`*` 应展开为一层子目录且按 Ordinal 排序");
}

#[test]
fn double_star_is_recursive_and_includes_base() {
    let root = tree("dstar", &["app", "deep/x/y"]);
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../deep/**")]);
    assert!(got.contains(&root.join("deep")), "`**` 应含基目录自身，实得 {got:?}");
    assert!(got.contains(&root.join("deep/x")), "应含中间层");
    assert!(got.contains(&root.join("deep/x/y")), "应递归到底");
}

#[test]
fn files_are_not_returned_only_dirs() {
    // zpkg 是**在目录里**按文件名找的，展开结果必须是目录。
    let root = tree("files", &["app", "mix"]);
    std::fs::write(root.join("mix/afile.txt"), b"x").unwrap();
    std::fs::create_dir_all(root.join("mix/adir")).unwrap();
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../mix/*")]);
    assert_eq!(got, vec![root.join("mix/adir")], "只应返回目录");
}

#[test]
fn duplicates_are_deduped_keeping_first() {
    let root = tree("dedup", &["app", "shared"]);
    let entry = root.join("app");
    let got = expand_probing_paths(&entry, &[PathBuf::from("../shared"), PathBuf::from("../shared")]);
    assert_eq!(got.len(), 1, "重复项应去重，实得 {got:?}");
}

#[test]
fn empty_patterns_yield_nothing() {
    // 没配 = 搜索序保持 [entry-dir, libs]，不能凭空多出目录。
    let root = tree("empty", &["app"]);
    let got = expand_probing_paths(&root.join("app"), &[]);
    assert!(got.is_empty());
}

// ── `${Z42_HOME}` 占位符（relocate-compiler-domain-libs §5.3.1）─────────────────
//
// 用可注入的 `expand_probing_paths_with`：候选根来自环境变量与 `current_exe()`，
// 测试里改 env 会彼此干扰（cargo 多线程），注入才测得准。

#[test]
fn z42_home_placeholder_expands_to_the_sdk_root() {
    let root = tree("home", &["app", "sdk/programs/z42c"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(
        &entry,
        &[PathBuf::from("${Z42_HOME}/programs/z42c")],
        &[root.join("sdk")],
    );
    assert_eq!(got, vec![root.join("sdk/programs/z42c")]);
}

#[test]
fn z42_home_placeholder_is_substituted_before_the_absolute_check() {
    // 要害：`${Z42_HOME}/…` 以 `$` 开头，对 `is_absolute()` 是**相对**路径。若先判再替换，
    // 它会被拼成 `<app>/${Z42_HOME}/…` —— 一个永远不存在的目录，然后静默跳过（配了等于没配）。
    // 这里 entry 目录下**故意**放一棵同名的假树：替换顺序错了就会命中它，测试立刻分得清。
    let root = tree("order", &["app/${Z42_HOME}/programs/z42c", "sdk/programs/z42c"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(
        &entry,
        &[PathBuf::from("${Z42_HOME}/programs/z42c")],
        &[root.join("sdk")],
    );
    assert_eq!(got, vec![root.join("sdk/programs/z42c")], "必须解析到 SDK 根，而不是 entry 目录下的同名树");
}

#[test]
fn z42_home_candidate_roots_keep_their_priority_order() {
    // 多个候选根都存在时，顺序 = 候选根的优先级（$Z42_HOME → $Z42_PORTABLE_VM 反推 → current_exe 反推）。
    // 先汇总再排序会把这个顺序洗成字典序 —— 所以排序必须发生在**每个根的展开内部**。
    let root = tree("prio", &["app", "b-sdk/programs/z42c", "a-sdk/programs/z42c"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(
        &entry,
        &[PathBuf::from("${Z42_HOME}/programs/z42c")],
        &[root.join("b-sdk"), root.join("a-sdk")],
    );
    assert_eq!(got, vec![root.join("b-sdk/programs/z42c"), root.join("a-sdk/programs/z42c")]);
}

#[test]
fn z42_home_placeholder_composes_with_globs() {
    let root = tree("homeglob", &["app", "sdk/programs/z42c", "sdk/programs/z42i"]);
    let entry = root.join("app");
    let got =
        expand_probing_paths_with(&entry, &[PathBuf::from("${Z42_HOME}/programs/*")], &[root.join("sdk")]);
    assert_eq!(got, vec![root.join("sdk/programs/z42c"), root.join("sdk/programs/z42i")]);
}

#[test]
fn unresolvable_z42_home_skips_the_entry_instead_of_faking_a_path() {
    // 没有任何候选根（既没设环境变量、也反推不出来）⇒ 这一条 pattern 消失，
    // 而不是变成一个含 `${…}` 的字面目录。
    let root = tree("nohome", &["app", "sdk/programs/z42c"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(&entry, &[PathBuf::from("${Z42_HOME}/programs/z42c")], &[]);
    assert!(got.is_empty(), "解析不出来就跳过，得到的是 {got:?}");
}

#[test]
fn unknown_placeholder_voids_the_whole_entry() {
    // 字面回落最坏：它会拼出 `<app>/${FOO}/x`，症状离原因更远。整条作废。
    let root = tree("unknown", &["app", "app/x"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(&entry, &[PathBuf::from("${FOO}/x")], &[root.join("app")]);
    assert!(got.is_empty());
}

#[test]
fn unclosed_placeholder_voids_the_whole_entry() {
    let root = tree("unclosed", &["app", "shared"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(&entry, &[PathBuf::from("${Z42_HOME/shared")], &[root.clone()]);
    assert!(got.is_empty());
}

#[test]
fn a_plain_entry_is_unaffected_by_the_placeholder_machinery() {
    // 绝大多数项不含 `${`：它们必须与占位符引入前逐字一致（这条守的是「没引入回归」）。
    let root = tree("plain", &["app", "shared"]);
    let entry = root.join("app");
    let got = expand_probing_paths_with(&entry, &[PathBuf::from("../shared")], &[root.join("nonexistent")]);
    assert_eq!(got, vec![root.join("shared")]);
}
