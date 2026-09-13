//! prelude（`z42.core.zpkg`）解析顺序（fix-prelude-entry-dir-resolution）。
//!
//! 钉的是一条**曾经不成立**的不变式：prelude 与其它依赖走同一个 colocated 顺序
//! ——entry zpkg 自己的目录优先，然后才是 `libs/`。
//!
//! 此前 `run` 的 5.1b 只看 `libs_dir` 一个目录，且首个版本不匹配就 `bail!`。于是
//! 「`libs/` 里是另一代的 z42.core、而 entry 目录里那份完全可读」会直接打死进程 ——
//! 两代自举的 gen2 步骤正是这个形状（旧 VM + entry-dir 同代 stdlib +
//! `Z42_LIBS` 指向刚写出的新一代 flat 视图），于是**任何** zbc/zpkg 格式 bump 都过不去。

use std::path::PathBuf;

use super::app::prelude_candidates;

/// 每个用例一个独立临时目录树（无 tempfile dev-dep，手工建/删）。
fn scratch(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("z42-prelude-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("scratch dir");
    p
}

fn touch_core(dir: &PathBuf) {
    std::fs::create_dir_all(dir).expect("dir");
    std::fs::write(dir.join("z42.core.zpkg"), b"not-a-real-zpkg").expect("write");
}

#[test]
fn entry_dir_prelude_comes_before_libs() {
    let root = scratch("order");
    let entry = root.join("entry");
    let libs = root.join("libs");
    touch_core(&entry);
    touch_core(&libs);

    let got = prelude_candidates(&[entry.clone(), libs.clone()]);
    assert_eq!(
        got,
        vec![entry.join("z42.core.zpkg"), libs.join("z42.core.zpkg")],
        "prelude must follow the colocated-deps order (entry dir first), not libs-only"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn libs_still_used_when_entry_dir_has_no_prelude() {
    // 常态：app 旁边没有 stdlib，全靠 libs/。顺序改动不得让这条退化。
    let root = scratch("libsonly");
    let entry = root.join("entry");
    let libs = root.join("libs");
    std::fs::create_dir_all(&entry).expect("dir");
    touch_core(&libs);

    let got = prelude_candidates(&[entry, libs.clone()]);
    assert_eq!(got, vec![libs.join("z42.core.zpkg")]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_candidate_is_offered_not_just_the_first() {
    // 关键：候选是**列表**而非单值。只要还有后续候选，前一个读失败就不该是终局
    // ——这正是 5.1b 从「首个不匹配即 bail」改成「全部失败才 bail」的依据。
    let root = scratch("multi");
    let a = root.join("a");
    let b = root.join("b");
    touch_core(&a);
    touch_core(&b);

    assert_eq!(prelude_candidates(&[a.clone(), b.clone()]).len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_prelude_yields_no_candidates() {
    let root = scratch("none");
    let entry = root.join("entry");
    std::fs::create_dir_all(&entry).expect("dir");
    assert!(prelude_candidates(&[entry]).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
