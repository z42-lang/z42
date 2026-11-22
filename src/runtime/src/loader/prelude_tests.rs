use super::prelude_candidates;
use std::path::PathBuf;

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

    let got = prelude_candidates(&crate::pal::fs::LocalFs, &[entry.clone(), libs.clone()]);
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

    let got = prelude_candidates(&crate::pal::fs::LocalFs, &[entry, libs.clone()]);
    assert_eq!(got, vec![libs.join("z42.core.zpkg")]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn every_prelude_candidate_is_offered() {
    // 关键：候选是**列表**而非单值。只要还有后续候选，前一个读失败就不该是终局
    // ——这正是 5.1b 从「首个不匹配即 bail」改成「全部失败才 bail」的依据。
    let root = scratch("multi");
    let a = root.join("a");
    let b = root.join("b");
    touch_core(&a);
    touch_core(&b);

    assert_eq!(
        prelude_candidates(&crate::pal::fs::LocalFs, &[a.clone(), b.clone()]).len(),
        2
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_prelude_yields_no_candidates() {
    let root = scratch("none");
    let entry = root.join("entry");
    std::fs::create_dir_all(&entry).expect("dir");
    assert!(prelude_candidates(&crate::pal::fs::LocalFs, &[entry]).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn prelude_falls_back_after_a_version_mismatch_and_reports_the_first_if_none_load() {
    use super::{load_prelude, PRELUDE_FILE};
    use crate::pal::fs::MemoryFs;
    use crate::zpkg::format::ZPKG_VERSION_MAJOR;

    let fs = MemoryFs::new();
    let entry = PathBuf::from("entry");
    let libs = PathBuf::from("libs");
    let candidate = entry.join(PRELUDE_FILE);
    let fallback = libs.join(PRELUDE_FILE);
    let package = include_bytes!(concat!(
        env!("Z42_RUNTIME_DIR"),
        "/../compiler/z42.package/tests/fixtures/zpkg-format/indexed-minimal/source.zpkg"
    ));
    let zbc = include_bytes!(concat!(
        env!("Z42_RUNTIME_DIR"),
        "/../compiler/z42.package/tests/fixtures/zpkg-format/indexed-minimal/source.zbc"
    ));
    let mut mismatched = package.to_vec();
    mismatched[4..6].copy_from_slice(&ZPKG_VERSION_MAJOR.wrapping_add(1).to_le_bytes());
    fs.mount(candidate.to_str().unwrap(), mismatched.clone());
    fs.mount(fallback.to_str().unwrap(), package.to_vec());
    fs.mount(libs.join("source.zbc").to_str().unwrap(), zbc.to_vec());

    let search_dirs = [entry, libs];
    let loaded = load_prelude(&fs, &search_dirs, None, false)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.path, fallback);

    fs.mount(fallback.to_str().unwrap(), mismatched);
    let error = load_prelude(&fs, &search_dirs, None, false)
        .err()
        .expect("version mismatch");
    assert!(error
        .to_string()
        .starts_with(&format!("`{}`", candidate.display())));
}
