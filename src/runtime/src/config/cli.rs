//! `--set <key>=<value>` 的解析：CLI 层（优先级链最高层）的入口。
//!
//! # 为什么是通用 `--set` 而不是每个旋钮一个 flag
//!
//! 31 个旋钮逐个加 clap flag 会让 `--help` 从 8 行涨到 40+，且每加一个旋钮要同时
//! 改 `Cli` struct、`KNOWN_KNOBS`、消费处三个地方——"表格是唯一 SoT"就破了。
//! 通用 `--set` 让新增旋钮的成本保持在一行表格编辑。
//!
//! # key 的形式
//!
//! 只认旋钮的完整 key（`toml_key`）与它**显式声明**的 `aliases`。**不**自动接受
//! `Z42_GC_MODE` 这种 env 名形式：那等于建立一条隐式的双写法约定，将来若某个
//! 旋钮的 env 名与 kebab key 不再是机械映射（为兼容改名），"自动等价"就会失效
//! 或产生歧义。别名要有，就在表里写出来、被 `--list-knobs` 打印出来。
//!
//! complete-runtime-settings P2（2026-09-05）。

use super::*;
use std::collections::BTreeMap;

/// 解析一组 `--set KEY=VALUE`，产出按**旋钮 env 名**索引的 CLI 层输入。
///
/// - 按**第一个** `=` 切分：`--set path=/a=b:/c` → key `path`, value `/a=b:/c`。
/// - 空值（`--set gc-mode=`）保留为空串——`resolve_knobs` 把它当"该层未设"，
///   于是回落下一层，即"显式清空"。
/// - 未知 key → `Err`，带最近邻建议。
/// - 同一 key 给多次 → **后者胜**（`-D` 类 flag 的通用约定，脚本拼接友好），
///   并 warn 出被覆盖的值。这与「`--mode` 和 `--set mode=` 同时给出就报错」不
///   矛盾：那是**两种拼写**争同一个旋钮，选谁都要用户记一条隐藏规则；这里是
///   同一种拼写重复，"后者胜"是不需要记的通用规则。
pub fn parse_set_args(args: &[String]) -> Result<BTreeMap<&'static str, String>, String> {
    let mut out: BTreeMap<&'static str, String> = BTreeMap::new();
    for arg in args {
        let Some((key, value)) = arg.split_once('=') else {
            return Err(format!(
                "z42: --set expects KEY=VALUE, got {arg:?}.\n     \
                 e.g. --set gc-mode=concurrent   (run `z42vm --list-knobs` for the key list)"
            ));
        };
        let key = key.trim();
        let Some(spec) = knob_by_key(key) else {
            return Err(unknown_key_error(key));
        };
        if let Some(prev) = out.insert(spec.name, value.to_string()) {
            eprintln!(
                "z42: --set {key}= given more than once; using {value:?} and ignoring {prev:?}"
            );
        }
    }
    Ok(out)
}

/// 未知 `--set` key 的报错——带最近邻建议。CLI 层的问题一律致命（用户此刻手敲
/// 的，静默忽略一个 typo 会让他以为设置生效了）。
fn unknown_key_error(key: &str) -> String {
    let mut msg = format!("z42: unknown runtime knob `{key}` in --set");
    if let Some(near) = suggest_key(key) {
        msg.push_str(&format!("; did you mean `{near}`?"));
    } else {
        msg.push('.');
    }
    msg.push_str("\n     Run `z42vm --list-knobs` (or `--list-knobs --all`) to see every knob.");
    msg
}

/// 最近的已知 key——编辑距离 ≤ 3 且不超过 key 长度的一半（够抓 typo，
/// 又不至于把风马牛不相及的输入配对成"你是不是想输…"）。
pub fn suggest_key(key: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for spec in KNOWN_KNOBS {
        let candidates = std::iter::once(spec.toml_key).chain(spec.aliases.iter().copied());
        for cand in candidates.filter(|c| !c.is_empty()) {
            let d = edit_distance(key, cand);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, cand));
            }
        }
    }
    let (d, cand) = best?;
    let limit = 3.min(key.len().div_ceil(2)).max(1);
    (d <= limit).then_some(cand)
}

/// Levenshtein 距离（两行滚动数组；key 都是短字符串，不值得引依赖）。
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// 专用 flag 与 `--set` 争同一个旋钮时报错。
///
/// 两者**同层**，谁也不比谁"更具体"——定义一个胜者只会制造记忆负担，所以要求
/// 用户只给一个。`flag_display` 是给用户看的 flag 名（如 `"--mode"`）。
pub fn reject_flag_conflict(
    set: &BTreeMap<&'static str, String>,
    env_name: &'static str,
    flag_display: &str,
    flag_value: Option<&str>,
) -> Result<(), String> {
    let (Some(from_set), Some(from_flag)) = (set.get(env_name), flag_value) else {
        return Ok(());
    };
    let key = knob_by_env_name(env_name).map_or(env_name, |k| k.toml_key);
    Err(format!(
        "z42: {flag_display} {from_flag} and --set {key}={from_set} both set the same knob \
         (they are the same precedence layer).\n     Pass only one of them."
    ))
}
