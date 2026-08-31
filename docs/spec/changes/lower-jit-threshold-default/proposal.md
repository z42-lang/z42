# Proposal: JIT tier-up 阈值默认 1000 → 1（编译器提速 ~12–17%）

## Why

JIT 函数级 tier-up 阈值 `jit_threshold`（`Z42_JIT_THRESHOLD`）默认 **1000**——函数被调 1000 次才编译，为**热循环
workload** 调（只编超热函数，冷尾留解释器省编译时间/代码页）。但 OSR（`osr_threshold`，back-edge）已独立处理热
**循环**；而一大类真实程序——**尤以 z42c 自举编译器**——时间花在**每个只调几次的函数**上：`_build` /
`PackageCompile.Compile` / `BuildPackageCus` / 整个 codegen+序列化管线每次 build 只跑 **1 次**。默认 1000 下它们
**永不编译 → 编译器全程解释执行**（`Z42_JIT_PROFILE` 实证：一次 build 只有 ~18 个被调 >1000 次的 leaf string util
被 JIT，编译器逻辑一个没编 → 默认 JIT 34.7s 甚至比纯 interp 34.0s 还慢，白付 18 个非瓶颈函数的编译开销）。

`N≥2` 无法帮到「只调 1 次」的函数——**只有 N=1（compile-on-first-call）能捕获它们**，无中间值。

## What Changes

- `src/runtime/src/jit/mod.rs`：`Z42_JIT_THRESHOLD` 默认 `unwrap_or(1000)` → `unwrap_or(1)`。
- 更新 `jit/mod.rs` 决策注释 + `jit/frame.rs` 过时注释（原写「default 2」，实际是 1000——一并改正为 1）。
- 无行为/格式/API 变更：JIT 对语义透明，产物 **byte-identical**（自举 gen1==gen2 守）。仅「哪些函数何时被编」变。

**实测（z42c.semantics 90 文件全量，本机）**：默认（无 env）34.7s → **30.5s（~12%）**，显式 `=1` 达 29.0s（~17%）；
byte-identical ✅；Cranelift 编译开销 <1% 采样（JIT 惰性/per-function，只有**到达**的函数才编，冷尾不预编）。

## Scope

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/runtime/src/jit/mod.rs` | MODIFY | 默认 `unwrap_or(1000)`→`unwrap_or(1)` + 决策注释 |
| `src/runtime/src/jit/frame.rs` | MODIFY | `jit_threshold` 文档注释「default 2」→「default 1」订正 |

**只读引用**：`src/runtime/src/jit/lazy_tests.rs`（显式设 `jit_threshold=2`/`=1` 测 tiering，**不受默认改动影响**，无需改）。

## Out of Scope

- ByteWriter 批量写、is_subclass memo（本调查另两处 byte-identical 微优化，独立 change；见 [[compiler-parallel-heavy-phases-investigation]]）。
- jit_call 桥优化（Arc 克隆 / call_stack 去锁——后者实测 0%，已弃）。
- 并行 lazy_loader arc-swap（正交，优先级低）。

## Open Questions

- [ ] 无。唯一决策点（全局默认 vs z42c 专属）User 已裁决 = **全局默认**（本 proposal），须过 bench 门验其它 workload 无回归。
