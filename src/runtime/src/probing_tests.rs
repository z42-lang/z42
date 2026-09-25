//! `expand_probing_paths` 的规则（add-deployment-model 批 3）。
//!
//! 守的是 design.md §probing-paths 的解析规则那张表——每条规则一个测试，而不是把它们
//! 混在一个"大概能用"的用例里：这些规则彼此独立，混在一起测就分不清是哪条坏了。

use crate::probing::expand_probing_paths;
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
