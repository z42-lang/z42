//! `available!(X)` 的加载期折叠与死分支剪枝（add-symbol-availability-macro）。
//!
//! # 为什么在加载期，而不是 JIT 期
//!
//! `available!(X)` 的值**只依赖符号表**——不执行任何用户代码、不依赖 `__static_init__`。
//! 因此它是全流程里**唯一**能在「模块已合并、但还没有任何东西执行过」这个窗口求值的常量。
//! 这个窗口的独占所有权（`&mut Module`，尚未进 `Arc`）让我们可以做**真正的原地 CFG 剪枝**：
//! 死分支的指令**物理消失**，interp 与 JIT 都再也看不到它。
//!
//! 对比 `[Invariant]` native 折叠（另一条线）：那个必须真的 call 进 dlopen 的代码，只能
//! first-call 惰性求值，届时 `Function` 已在 `Arc` 后不可变 → 只能常量化、不能剪 CFG。
//!
//! # 顺序约束（硬）
//!
//! ```text
//! merge_modules → build_type_registry → [fold_availability] → build_block_indices → build_func_index
//! ```
//!
//! 必须在 `build_block_indices` **之前**（剪枝会改块集合，派生侧表要按剪枝后的 CFG 建），
//! 且在**任何 token 解析之前**——被剪掉的分支里的 call site 因此永不进入
//! `resolve_function_tokens`，也就不会触发后续 `fix-silent-symbol-resolution` 的缺符号抛出。
//! 这正是 `available!` 存在的意义：它是那条急切校验的**唯一显式豁免通道**。
//!
//! # 判定策略（design D4）
//!
//! 1. 目标在已合并模块里 → true，零加载。
//! 2. 否则按 key 的 namespace 找**声明了该 ns 的候选 zpkg**，只 probe 那一个文件。
//! 3. 都不认领 → **确定不存在** → false，零加载。
//!
//! 加载放大因此有界（≤ `available!` 触及的不同 dep 文件数），不会退化成「加载全世界」。

use crate::metadata::bytecode::{Instruction, Module, Terminator};
use crate::metadata::namespace_index::ZpkgCandidate;
use std::collections::HashSet;

/// 编译器发射的探测 builtin 名（与 `TypeOpEmitter._emitSymAvailable` 对齐）。
pub const SYM_AVAILABLE: &str = "__sym_available";

/// key 前缀：方法 / 类型。与编译器侧一致。
const KEY_METHOD: &str = "m:";
const KEY_TYPE: &str = "t:";

/// 折叠统计。
///
/// **存在的理由不是好奇心**：剪枝发生在内存里、zbc 字节不变，没有任何外部可观测面。
/// 不暴露计数的话，「pass 根本没跑」和「跑了但没什么可折」在测试里长得一模一样——
/// 那就是一个从不打印、从不失败的门。测试必须同时断言「行为正确」**和**「确实折了」。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AvailabilityStats {
    /// 折成常量的 `__sym_available` 站点数。
    pub folded: usize,
    /// 判定为可用的站点数。
    pub resolved_true: usize,
    /// 判定为不可用的站点数。
    pub resolved_false: usize,
    /// 剪掉的不可达块数。
    pub blocks_removed: usize,
    /// 因函数带异常表而**只折不剪**的函数数（CFG 不含异常隐式边，剪块会破坏 handler 可达性）。
    pub funcs_kept_for_exceptions: usize,
}

impl AvailabilityStats {
    pub fn is_noop(&self) -> bool { self.folded == 0 && self.blocks_removed == 0 }
}

/// 加载期折叠 + 剪枝。返回统计供测试与 trace 断言。
///
/// `declared` 是「声明了但尚未加载」的候选 zpkg（与 lazy loader 用的是同一份）。
pub fn fold_availability(
    module: &mut Module,
    declared: &[(String, ZpkgCandidate)],
) -> AvailabilityStats {
    let mut stats = AvailabilityStats::default();

    // Fast path：绝大多数模块根本没有 available!()，整段跳过，零成本。
    if !module_has_probe(module) {
        return stats;
    }

    // probe 结果缓存：同一 key 在多处出现时只解析一次，也避免重复 probe 同一个 zpkg。
    let mut cache: std::collections::HashMap<String, bool> = std::collections::HashMap::new();

    for fi in 0..module.functions.len() {
        let folded_here = fold_probes_in_function(module, fi, declared, &mut cache, &mut stats);
        if folded_here == 0 {
            continue;
        }
        // 有异常表的函数只折不剪：z42 IR 的 CFG 只从终结子构建、**不含异常隐式边**，
        // 贸然移块会删掉 handler 可达的块 → miscompile。这条铁律与编译器侧
        // `IrDeadBranch`（ExcCount>0 时只折不移）逐字一致。
        if !module.functions[fi].exception_table().is_empty() {
            stats.funcs_kept_for_exceptions += 1;
            continue;
        }
        fold_constant_branches(module, fi);
        stats.blocks_removed += remove_unreachable_blocks(module, fi);
    }

    stats
}

/// 模块里是否出现过探测 builtin。
fn module_has_probe(module: &Module) -> bool {
    module.functions.iter().any(|f| {
        f.blocks.iter().any(|b| {
            b.instructions.iter().any(|i| matches!(
                i, Instruction::Builtin(bi) if bi.name == SYM_AVAILABLE
            ))
        })
    })
}

/// 把一个函数里所有 `__sym_available` 站点折成 `ConstBool`。返回折叠数。
///
/// key 的来源：探测 builtin 的唯一实参是一个由**同块内** `ConstStr` 定义的寄存器
/// （编译器发射时紧邻产出，见 `TypeOpEmitter._emitSymAvailable`）。这里做块内单赋值扫描，
/// 与编译器侧 `IrDeadBranch._foldBranches` 收集 `ConstBoolInstr` 的手法同形。
fn fold_probes_in_function(
    module: &mut Module,
    fi: usize,
    declared: &[(String, ZpkgCandidate)],
    cache: &mut std::collections::HashMap<String, bool>,
    stats: &mut AvailabilityStats,
) -> usize {
    let mut folded = 0usize;

    for bi in 0..module.functions[fi].blocks.len() {
        // 块内 reg → 字符串池 idx（ConstStr 的单赋值定义）。
        let mut const_str: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let n = module.functions[fi].blocks[bi].instructions.len();

        for ii in 0..n {
            match &module.functions[fi].blocks[bi].instructions[ii] {
                Instruction::ConstStr { dst, idx } => {
                    const_str.insert(u32::from(*dst), *idx);
                }
                Instruction::Builtin(b) if b.name == SYM_AVAILABLE => {
                    let dst = b.dst;
                    let Some(arg0) = b.args.first().copied() else { continue };
                    let Some(&sidx) = const_str.get(&u32::from(arg0)) else {
                        // 参数不是同块常量串 —— 编译器不会产生这种形态。保守跳过（不折不剪），
                        // 运行期回落到 builtin 本身（它会在 debug 下告警）。
                        tracing::debug!("available!: probe arg is not a same-block ConstStr; skipped");
                        continue;
                    };
                    let Some(key) = module.string_pool.get(sidx as usize).cloned() else { continue };

                    let val = match cache.get(&key) {
                        Some(&v) => v,
                        None => {
                            let v = resolve_key(module, &key, declared);
                            cache.insert(key.clone(), v);
                            v
                        }
                    };
                    if val { stats.resolved_true += 1; } else { stats.resolved_false += 1; }
                    tracing::debug!("available!: `{key}` -> {val}");

                    module.functions[fi].blocks[bi].instructions[ii] =
                        Instruction::ConstBool { dst, val };
                    folded += 1;
                }
                _ => {}
            }
        }
    }

    stats.folded += folded;
    folded
}

/// 判定一个 key 是否可解析。
fn resolve_key(module: &Module, key: &str, declared: &[(String, ZpkgCandidate)]) -> bool {
    if let Some(fq) = key.strip_prefix(KEY_METHOD) {
        if module.func_index.contains_key(fq) { return true; }
        return probe_declared(declared, fq, ProbeKind::Method);
    }
    if let Some(fq) = key.strip_prefix(KEY_TYPE) {
        if module.type_registry.contains_key(fq) { return true; }
        return probe_declared(declared, fq, ProbeKind::Type);
    }
    // 未知前缀 = 编译器与 VM 失配。保守当作「不可用」并留痕，而不是静默 true——
    // true 会让代码走进一条它以为存在的分支，错得更远。
    tracing::warn!("available!: unknown key prefix in `{key}`; treating as unavailable");
    false
}

#[derive(Clone, Copy)]
enum ProbeKind { Method, Type }

/// 按 namespace 定向 probe 尚未加载的候选 zpkg（design D4 第 2/3 步）。
///
/// **只加载认领了该 namespace 的那些文件**，不做全量加载。probe 是只读的：加载出来的
/// artifact 用完即弃，不并入运行模块——回答存在性不需要副作用。
fn probe_declared(declared: &[(String, ZpkgCandidate)], fq: &str, kind: ProbeKind) -> bool {
    let Some(ns) = namespace_of(fq) else { return false };

    // 稳定序 + 去重：候选来自文件系统枚举，顺序不确定（common-pitfalls §1）。probe 只回答
    // yes/no，顺序不影响结果，但**遍历顺序确定**能让 trace 与失败复现稳定。
    let mut files: Vec<&ZpkgCandidate> = declared
        .iter()
        .filter(|(_, cand)| cand.namespaces.iter().any(|c| claims_namespace(c, &ns)))
        .map(|(_, cand)| cand)
        .collect();
    files.sort_by(|a, b| a.file_path.cmp(&b.file_path));

    let mut seen: HashSet<&std::path::Path> = HashSet::new();
    for cand in files {
        if !seen.insert(cand.file_path.as_path()) { continue; }
        let path = cand.file_path.to_string_lossy().into_owned();
        let Ok(art) = crate::metadata::load_artifact(&path) else {
            // probe 失败不是错误：文件可能损坏/版本不符，此时「不可用」正是我们要的答案。
            tracing::debug!("available!: probe of `{path}` failed; treating as unavailable");
            continue;
        };
        let hit = match kind {
            ProbeKind::Method => art.module.func_index.contains_key(fq),
            ProbeKind::Type => art.module.type_registry.contains_key(fq),
        };
        if hit { return true; }
    }
    false
}

/// `A.B.C.Member` → `A.B.C`（去掉最后一段）。无点 → None（全局命名空间，无候选可查）。
fn namespace_of(fq: &str) -> Option<String> {
    fq.rfind('.').map(|i| fq[..i].to_string())
}

/// 候选声明的 namespace `claimed` 是否覆盖目标 `ns`（等于或为其祖先）。
fn claims_namespace(claimed: &str, ns: &str) -> bool {
    ns == claimed || ns.starts_with(&format!("{claimed}."))
}

/// `BrCond`(常量 cond) → `Br`。喂料是上一步折出来的 `ConstBool`。
fn fold_constant_branches(module: &mut Module, fi: usize) {
    // 全函数收集单赋值 ConstBool（多次赋值的寄存器不可信 → 排除）。
    let mut val: std::collections::HashMap<u32, bool> = std::collections::HashMap::new();
    let mut multi: HashSet<u32> = HashSet::new();
    for b in &module.functions[fi].blocks {
        for i in &b.instructions {
            if let Some(d) = i.written_reg() {
                if let Instruction::ConstBool { val: v, .. } = i {
                    if val.insert(d, *v).is_some() { multi.insert(d); }
                } else if val.contains_key(&d) {
                    multi.insert(d);
                }
            }
        }
    }
    for d in &multi { val.remove(d); }

    for b in &mut module.functions[fi].blocks {
        let Terminator::BrCond { cond, true_label, false_label } = &b.terminator else { continue };
        let Some(&v) = val.get(&u32::from(*cond)) else { continue };
        let taken = if v { true_label.clone() } else { false_label.clone() };
        b.terminator = Terminator::Br { label: taken };
    }
}

/// 从 entry(block 0) 沿终结子做可达性 BFS，移除不可达块。返回移除数。
///
/// 安全性：有效 IR 里 def 支配 use → 可达块引用的寄存器其定义块必可达，移不可达块不产生悬垂读。
/// 调用方已保证本函数无异常表。
fn remove_unreachable_blocks(module: &mut Module, fi: usize) -> usize {
    let nblocks = module.functions[fi].blocks.len();
    if nblocks == 0 { return 0; }

    let index_of = |label: &str, f: &crate::metadata::bytecode::Function| -> Option<usize> {
        f.blocks.iter().position(|b| b.label == label)
    };

    let mut reach = vec![false; nblocks];
    let mut stack = vec![0usize];
    reach[0] = true;
    while let Some(bi) = stack.pop() {
        let succs: Vec<String> = match &module.functions[fi].blocks[bi].terminator {
            Terminator::Br { label } => vec![label.clone()],
            Terminator::BrCond { true_label, false_label, .. } =>
                vec![true_label.clone(), false_label.clone()],
            _ => Vec::new(),
        };
        for s in succs {
            if let Some(t) = index_of(&s, &module.functions[fi]) {
                if !reach[t] { reach[t] = true; stack.push(t); }
            }
        }
    }

    let keep = reach.iter().filter(|r| **r).count();
    if keep == nblocks { return 0; }

    let mut it = reach.iter();
    module.functions[fi].blocks.retain(|_| *it.next().unwrap());
    nblocks - keep
}

#[cfg(test)]
mod tests {
    use super::*;

    // namespace_of：key 的属主 namespace = 去掉最后一段。
    #[test]
    fn namespace_of_strips_last_segment() {
        assert_eq!(namespace_of("A.B.C.Member").as_deref(), Some("A.B.C"));
        assert_eq!(namespace_of("A.Member").as_deref(), Some("A"));
    }

    // 无点 = 全局命名空间：没有候选包能「认领」它，probe 无从下手 → None。
    #[test]
    fn namespace_of_bare_name_is_none() {
        assert_eq!(namespace_of("Member"), None);
    }

    // 候选声明 `A.B` 覆盖 `A.B` 与其子 ns，但**不覆盖** `A.BC`（前缀相似不等于父子）。
    #[test]
    fn claims_namespace_matches_self_and_descendants() {
        assert!(claims_namespace("A.B", "A.B"));
        assert!(claims_namespace("A.B", "A.B.C"));
        assert!(!claims_namespace("A.B", "A"));
        assert!(!claims_namespace("A.B", "A.BC"));
        assert!(!claims_namespace("A.B", "X.B"));
    }

    // 统计量的 is_noop 决定 app.rs 是否重建派生侧表——折了或剪了都必须非 noop。
    #[test]
    fn stats_is_noop_only_when_nothing_happened() {
        assert!(AvailabilityStats::default().is_noop());
        let folded = AvailabilityStats { folded: 1, ..Default::default() };
        assert!(!folded.is_noop());
        let pruned = AvailabilityStats { blocks_removed: 1, ..Default::default() };
        assert!(!pruned.is_noop());
    }
}
