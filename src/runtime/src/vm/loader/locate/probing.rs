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
///   * entries that do not exist are skipped silently (an optional plugin dir is fine);
///   * `${Z42_HOME}` 展开成本机 SDK 根（见下方占位符一节）——**在相对/绝对判定之前**替换；
///     未知或未闭合的 `${…}` 让**整条** pattern 作废（不做字面回落）。
pub fn expand_probing_paths(entry_dir: &std::path::Path, patterns: &[PathBuf]) -> Vec<PathBuf> {
    expand_probing_paths_with(entry_dir, patterns, &z42_home_roots())
}

/// [`expand_probing_paths`] 的纯形式：`${Z42_HOME}` 的候选根由调用方注入（测试用）。
///
/// 分出这一层是因为根来自**进程全局**（环境变量 + `current_exe()`）——测试若去改 env 就
/// 彼此干扰（cargo 默认多线程跑）。同 `hostrun.rs::resolve_app_runtime_in` 的处理。
///
/// 🔴 **替换必须在「这是绝对路径吗」之前**：`${Z42_HOME}/programs/z42c` 以 `$` 开头，
/// 对 `is_absolute()` 是**相对**路径，先判就会被拼到 entry 目录后面去，得到一个
/// 永远不存在的 `<app>/${Z42_HOME}/programs/z42c`，然后静默跳过 —— 症状是「配了等于没配」。
pub fn expand_probing_paths_with(
    entry_dir: &std::path::Path,
    patterns: &[PathBuf],
    z42_home_roots: &[PathBuf],
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for pat in patterns {
        // 一个 pattern 可展开成 0..n 个具体 pattern（0 = 占位符解析不出来 ⇒ 整条跳过）。
        // 逐个 glob 并**在每个内部排序**，而不是先汇总再排：那样会把候选根的优先级顺序洗掉。
        for concrete in substitute_placeholders(pat, z42_home_roots) {
            let joined =
                if concrete.is_absolute() { concrete } else { normalize_lexically(&entry_dir.join(concrete)) };
            let has_glob = joined.components().any(|c| c.as_os_str().to_string_lossy().contains('*'));
            let mut hits = if has_glob { glob_dirs(&joined) } else { vec![joined] };
            hits.sort();
            for h in hits {
                if h.is_dir() && !out.contains(&h) {
                    out.push(h);
                }
            }
        }
    }
    out
}

// ── `${…}` 占位符（relocate-compiler-domain-libs §5.3.1）─────────────────────────
//
// **编译输出里不许烤具体路径**：侧车 `probing-paths` 随产物分发，绝对路径换台机器/换安装位置
// 就失效，而「相对 entry 目录」对「装在任意位置的用户应用要指回 SDK」无解。⇒ 侧车写
// `${Z42_HOME}/programs/z42c`，由 VM 在解析时换成本机真实目录。
//
// **谁来写这个占位符：作者，不是 z42c。** 落这条 support 时我挂过一条阶段-2 欠账
// （「等 nightly 成种子后让 z42c 发射占位符而不是具体路径」），**那条欠账的前提是错的、已撤**：
// 实测 z42c **从不合成 probing-paths** —— 侧车里的值是清单 `[profile.<n>.runtime]` 里作者写
// 什么就逐字写出（`ManifestLoader` 只把数组用 `\n` 连起来），全仓构建输出里今天一个绝对路径
// 都没有。所以没有「具体路径」需要被替换，也就没有 use 侧要等 nightly：作者今天写
// `${Z42_HOME}/programs/z42c`，这里就展开它。
// 端到端覆盖在 z42c.driver 的 CLI 夹具 `probing-paths-z42-home`（清单 → 侧车仍是占位符
// → VM 展开到 SDK 根；不设 `Z42_HOME` 必须跑不起来，否则那格失去判别力）。
//
// **大小写不是随手写的**，两层各有各的约定：
//   · `${lower_snake}` = **清单模板变量**，编译期展开（`PathTemplate` 的 `${workspace_dir}`
//     等）——作者写在清单里的东西；
//   · `${UPPER}`       = **环境派生的根**，运行期展开（本处）——名字与它来源的环境变量一致。
//
// **未知/未闭合占位符 ⇒ 整条 pattern 跳过**，不做字面回落。这一条与编译期那套宏**刻意不同**
// （`ExeDeps._rejectDepPathMacro` 对依赖 path 里的宏**硬报错**）：编译期有诊断通道、错了要当场说清；
// 而 VM 在这里没有不污染输出的通道（golden 判定把 stderr 并进 stdout，任何 WARN 都会波及全部
// golden），且 probing 项的既有语义本就是「不存在就静默跳过」。字面回落最坏——它会拼出
// `<app>/${FOO}/x` 这种谁也找不到的目录，症状离原因更远。

/// pattern 里认得的占位符名。今天只有一个。
const PLACEHOLDER_Z42_HOME: &str = "Z42_HOME";

/// 把一个可能含 `${…}` 的 pattern 展开成 0..n 个具体 pattern。
///
/// 无 `${` → 原样一条（绝大多数项走这里，零开销）；`${Z42_HOME}` → 每个候选根一条，
/// **保持候选根的顺序**；未知名或未闭合 → 空（整条跳过）。
fn substitute_placeholders(pat: &std::path::Path, z42_home_roots: &[PathBuf]) -> Vec<PathBuf> {
    let s = pat.to_string_lossy().to_string();
    if !s.contains("${") {
        return vec![pat.to_path_buf()];
    }
    // 先验一遍：只要出现任何一个不认识的（或未闭合的）占位符，整条作废。
    let mut rest = s.as_str();
    while let Some(at) = rest.find("${") {
        let after = &rest[at + 2..];
        match after.find('}') {
            None => return Vec::new(), // 未闭合
            Some(end) => {
                if &after[..end] != PLACEHOLDER_Z42_HOME {
                    return Vec::new(); // 未知名
                }
                rest = &after[end + 1..];
            }
        }
    }
    let token = format!("${{{PLACEHOLDER_Z42_HOME}}}");
    z42_home_roots
        .iter()
        .map(|root| PathBuf::from(s.replace(&token, &root.to_string_lossy())))
        .collect()
}

/// `${Z42_HOME}` 的候选根，**按优先级**（去重；空/取不到的档跳过）：
///   ① `$Z42_HOME`         —— 显式指定的安装位置（launcher 转发时已设）
///   ② `$Z42_PORTABLE_VM`  —— 反推 SDK 根（`<root>/bin/z42vm` ⇒ 上两级）；apphost 启动前会设它
///   ③ `current_exe()`     —— 正在跑的这个 z42vm 自己的位置，同样上两级
///
/// ③ 是兜底而非多余：两个环境变量都没设时（直接 `z42vm app.zpkg`），VM 仍然知道自己住在哪。
/// 展开结果**必须是真实存在的目录**才会进搜索序（`expand_probing_paths_with` 的既有过滤），
/// 所以反推错了的档自然落空，不会变成一个假目录。
fn z42_home_roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !p.as_os_str().is_empty() && !out.contains(&p) {
            out.push(p);
        }
    };
    if let Some(h) = env_nonempty("Z42_HOME") {
        push(PathBuf::from(h));
    }
    if let Some(vm) = env_nonempty("Z42_PORTABLE_VM") {
        if let Some(root) = sdk_root_of_vm(std::path::Path::new(&vm)) {
            push(root);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = sdk_root_of_vm(&exe) {
            push(root);
        }
    }
    out
}

fn env_nonempty(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// `<root>/bin/z42vm` → `<root>`（安装态与便携包都是这个形状）。
fn sdk_root_of_vm(vm: &std::path::Path) -> Option<PathBuf> {
    vm.parent()?.parent().map(|p| p.to_path_buf())
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

// ── 本进程的 zpkg 依赖搜索目录（对 z42 侧只读暴露）────────────────────────────
//
// `app.rs` 算好的那份 `search_dirs` 是个局部变量，z42 代码看不见它 —— 而「这个进程会去哪些
// 目录找 zpkg」在 z42 侧今天**完全无解**，于是长出了四档硬编码探测
// （`builder.z42::_findCompilerZpkg` 与 `ReplCompilerHost::_findCompilerZpkg` 两份平行实现），
// 以及 runtime-only 包里那个恒失败的 scripting 空壳 —— 四条路径全落空，却说不出为什么。
//
// 存成 OnceLock（同 `runtime_config()`）：boot 期写一次、之后只读。**相对项已按 entry zpkg
// 解析、通配符已展开、不存在的已剔除** —— 暴露的是结果，不是配置，调用方不需要重做一遍
// 那套规则（重做就会漂移）。
static SEARCH_DIRS: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();

// ── `${Z42_HOME}` 解析不到时的提示（add-sdk-libs D6）──────────────────────────────
//
// `${Z42_HOME}/…` 条目展开后一个目录都不存在 ⇒ 按既有规则**静默跳过**（可选插件目录不该让启动
// 失败）。但这类条目几乎都是「指回 SDK」的（`deploy = "sdk"` 由 z42c 自动补的就是
// `${Z42_HOME}/programs/z42c`），跳过之后用户看到的只是一句泛泛的「找不到符号 / 找不到 zpkg」，
// 原因（这台机器没装 SDK，或只装了 runtime）离症状很远。
//
// 所以启动时把这类条目**原样**记下来，依赖解析失败的报错点（`make_missing_symbol_exception`、
// `NamespaceCandidate::build_in_dirs`）有记录就附一句提示；没有记录 ⇒ 报错照旧。

/// `patterns` 里含 `${Z42_HOME}`、且用 `z42_home_roots` 展开后**一个存在的目录都没有**的条目（原样字符串）。
pub fn unresolved_z42_home_patterns_with(
    entry_dir: &std::path::Path,
    patterns: &[PathBuf],
    z42_home_roots: &[PathBuf],
) -> Vec<String> {
    let token = format!("${{{PLACEHOLDER_Z42_HOME}}}");
    patterns
        .iter()
        .filter(|p| p.to_string_lossy().contains(&token))
        .filter(|p| expand_probing_paths_with(entry_dir, std::slice::from_ref(*p), z42_home_roots).is_empty())
        .map(|p| p.to_string_lossy().to_string())
        .collect()
}

/// [`unresolved_z42_home_patterns_with`] 用进程的真实候选根。
pub fn unresolved_z42_home_patterns(entry_dir: &std::path::Path, patterns: &[PathBuf]) -> Vec<String> {
    unresolved_z42_home_patterns_with(entry_dir, patterns, &z42_home_roots())
}

/// 提示文本；`unresolved` 为空 ⇒ `None`（不附提示）。
pub fn format_sdk_missing_hint(unresolved: &[String]) -> Option<String> {
    if unresolved.is_empty() {
        return None;
    }
    Some(format!(
        "probing 路径 {} 无法解析 —— 是否没有安装 z42 SDK？（安装 SDK，或设置 Z42_HOME 指向 SDK 根目录）",
        unresolved.join(", ")
    ))
}

static UNRESOLVED_SDK_PATTERNS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

/// boot 期由 `app.rs` 写入（与 [`set_search_dirs`] 同一处）。重复调用忽略。
pub fn set_unresolved_sdk_patterns(patterns: Vec<String>) {
    let _ = UNRESOLVED_SDK_PATTERNS.set(patterns);
}

/// 依赖解析失败时附在报错后的提示；本进程没有解析不到的 `${Z42_HOME}` 条目 ⇒ `None`。
pub fn sdk_missing_hint() -> Option<String> {
    format_sdk_missing_hint(UNRESOLVED_SDK_PATTERNS.get().map(|v| v.as_slice()).unwrap_or(&[]))
}

/// 把提示（若有）接在 `msg` 后面。
pub fn with_sdk_missing_hint(msg: String) -> String {
    match sdk_missing_hint() {
        Some(h) => format!("{msg}\n  {h}"),
        None => msg,
    }
}

/// boot 期由 `app.rs` 写入解析结果。重复调用忽略（OnceLock 语义）。
pub fn set_search_dirs(dirs: Vec<PathBuf>) {
    let _ = SEARCH_DIRS.set(dirs);
}

/// 解析 zpkg 依赖时**按序**查找的目录。未初始化（非 app 路径，如纯 host 嵌入）→ 空。
pub fn search_dirs() -> &'static [PathBuf] {
    SEARCH_DIRS.get().map(|v| v.as_slice()).unwrap_or(&[])
}
