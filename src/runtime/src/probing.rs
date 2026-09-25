//! zpkg 依赖的**额外搜索目录**展开（add-deployment-model 批 3）。
//!
//! `Z42_PROBING_PATHS` / `[runtime] probing-paths` 声明的每一项在**解析时**展开成具体目录，
//! 由 `app.rs` 的 `search_dirs` 消费（entry-dir 之后、libs 之前）。
//!
//! 放 lib 而不是 bin 的 `startup.rs`：唯一消费者 `app.rs` 就在 lib 里，bin 够不着。

use std::path::PathBuf;

/// Expand `Z42_PROBING_PATHS` into concrete directories for dependency resolution
/// (add-deployment-model 批 3).
///
/// Rules (design.md §probing-paths 的解析规则):
///   * relative entries resolve against **the entry zpkg's directory**, not cwd —
///     the same install must behave identically whatever directory it is run from;
///   * absolute entries are used as-is;
///   * `*` matches one path segment, `**` any depth — expansion yields **directories**
///     (a dep zpkg is then looked up by file name inside each);
///   * expansion happens at resolution time, so plugin dirs added after the build are
///     still found;
///   * results are sorted (Ordinal) per pattern — never rely on readdir order, or a
///     zpkg present in two dirs resolves nondeterministically;
///   * entries that do not exist are skipped silently (an optional plugin dir is fine).
pub fn expand_probing_paths(entry_dir: &std::path::Path, patterns: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for pat in patterns {
        let joined = if pat.is_absolute() { pat.clone() } else { normalize_lexically(&entry_dir.join(pat)) };
        let has_glob = joined.components().any(|c| c.as_os_str().to_string_lossy().contains('*'));
        let mut hits = if has_glob { glob_dirs(&joined) } else { vec![joined] };
        hits.sort();
        for h in hits {
            if h.is_dir() && !out.contains(&h) {
                out.push(h);
            }
        }
    }
    out
}

/// Fold `.` and `..` **lexically** (no filesystem access, symlinks untouched).
///
/// Without this, `entry_dir.join("../shared")` stays literally `app/../shared`: it still
/// *resolves* to the right directory, but it is a different **string** from `shared`, so
/// the de-dup in `expand_probing_paths` (and in `search_dirs`) would keep both — the same
/// directory searched twice, and unreadable in diagnostics. Lexical (not `canonicalize`)
/// because canonicalising would also resolve symlinks, which changes what the user wrote.
fn normalize_lexically(p: &std::path::Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // Only pop a real segment; keep `..` that would escape the root as-is.
                let popped = out
                    .components()
                    .next_back()
                    .map(|c| !matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
                    .unwrap_or(false);
                if popped {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            c => out.push(c.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// Expand a glob pattern to existing **directories**. Supports `*` (one segment) and
/// `**` (any depth). Walks component by component so no external crate is needed and the
/// traversal stays bounded by what actually exists on disk.
///
/// 🔴 走 `Path::components()` 而**不是**按分隔符切字符串：Windows 的绝对路径是 `C:\...`，
/// 按 `MAIN_SEPARATOR` 切出来的首段是 `C:` 而不是空串，于是「这是绝对路径吗」判错、
/// 拼出 `.\C:\...` 这种谁也找不到的东西。`components()` 把盘符前缀（`Prefix`）与根
/// （`RootDir`）作为独立分量给出来，三个平台同一套代码。
fn glob_dirs(pattern: &std::path::Path) -> Vec<PathBuf> {
    use std::path::Component;
    let mut comps = pattern.components().peekable();
    // 先吃掉前缀与根（Windows: `C:` + `\`；unix: `/`），它们不参与匹配。
    let mut base = PathBuf::new();
    while let Some(c) = comps.peek() {
        match c {
            Component::Prefix(_) | Component::RootDir => {
                base.push(c.as_os_str());
                comps.next();
            }
            _ => break,
        }
    }
    if base.as_os_str().is_empty() {
        base = PathBuf::from(".");
    }
    let mut current: Vec<PathBuf> = vec![base];
    for comp in comps {
        let seg = comp.as_os_str().to_string_lossy().to_string();
        if seg.is_empty() || seg == "." {
            continue;
        }
        let mut next: Vec<PathBuf> = Vec::new();
        for b in &current {
            if seg == "**" {
                collect_dirs_recursive(b, &mut next);
            } else if seg.contains('*') {
                let mut kids = read_dir_names(b);
                kids.sort();
                for name in kids {
                    if glob_segment_matches(&seg, &name) {
                        next.push(b.join(name));
                    }
                }
            } else {
                let p = b.join(&seg);
                if p.exists() {
                    next.push(p);
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    current.into_iter().filter(|p| p.is_dir()).collect()
}

/// `base` itself plus every directory beneath it (for `**`).
fn collect_dirs_recursive(base: &std::path::Path, out: &mut Vec<PathBuf>) {
    if !base.is_dir() {
        return;
    }
    out.push(base.to_path_buf());
    let mut names = read_dir_names(base);
    names.sort();
    for name in names {
        let child = base.join(name);
        if child.is_dir() {
            collect_dirs_recursive(&child, out);
        }
    }
}

fn read_dir_names(dir: &std::path::Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Match one path segment against a pattern containing `*` (which matches any run of
/// characters **within** the segment). Plain backtracking — patterns are tiny.
fn glob_segment_matches(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let n: Vec<char> = name.chars().collect();
    fn go(p: &[char], n: &[char]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some('*') => (0..=n.len()).any(|i| go(&p[1..], &n[i..])),
            Some(c) => !n.is_empty() && n[0] == *c && go(&p[1..], &n[1..]),
        }
    }
    go(&p, &n)
}
