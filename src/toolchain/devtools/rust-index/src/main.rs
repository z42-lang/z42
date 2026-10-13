//! z42-rust-index：把 Rust crate 的条目与 rustdoc 写成 `z42 doc` 的外部索引文件。
//!
//! 用法：`z42-rust-index --root <仓库根> --out <索引文件> [--regen <重建命令>] <crate 名>=<相对根的 crate 根文件> ...`
//!
//! 从每个 crate 根文件出发，沿 `mod` 声明走模块树（`#[path]` 照认，`#[cfg(test)]` 模块跳过），
//! 所以条目的模块路径与 rustc 看到的一致，测试代码不进索引。输出格式见 [`out`]。

mod items;
mod out;
mod text;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut regen = String::new();
    let mut crates: Vec<(String, PathBuf)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" if i + 1 < args.len() => { root = Some(PathBuf::from(&args[i + 1])); i += 2; }
            "--out" if i + 1 < args.len() => { out_path = Some(PathBuf::from(&args[i + 1])); i += 2; }
            "--regen" if i + 1 < args.len() => { regen = args[i + 1].clone(); i += 2; }
            a => match a.split_once('=') {
                Some((name, file)) => { crates.push((name.to_string(), PathBuf::from(file))); i += 1; }
                None => return usage(&format!("unexpected argument `{a}`")),
            },
        }
    }
    let (Some(root), Some(out_path)) = (root, out_path) else { return usage("--root and --out are required"); };
    if crates.is_empty() { return usage("give at least one <crate>=<root file>"); }

    let mut w = out::Writer::new(&regen);
    for (name, rel) in &crates {
        let mut walker = items::Walker::new(&root, name, &mut w);
        if let Err(e) = walker.crate_root(&root.join(rel)) {
            eprintln!("z42-rust-index: {name}: {e}");
            return ExitCode::from(1);
        }
    }
    if let Some(dir) = out_path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("z42-rust-index: cannot create {}: {e}", dir.display());
            return ExitCode::from(1);
        }
    }
    if let Err(e) = std::fs::write(&out_path, w.finish()) {
        eprintln!("z42-rust-index: cannot write {}: {e}", out_path.display());
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("z42-rust-index: {msg}");
    eprintln!("usage: z42-rust-index --root <dir> --out <file> [--regen <cmd>] <crate>=<root.rs> ...");
    ExitCode::from(2)
}

/// `root` 下的相对路径，`/` 分隔（索引里的文件位置一律相对仓库根）。
pub(crate) fn rel_path(root: &Path, p: &Path) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}
