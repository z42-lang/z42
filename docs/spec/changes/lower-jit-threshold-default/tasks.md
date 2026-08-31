# Tasks: JIT tier-up 阈值默认 1000 → 1

> 状态：🟡 进行中 | 创建：2026-08-31 | 类型：perf（byte-identical，JIT 对语义透明）

## 阶段 1: 实现
- [x] 1.1 `jit/mod.rs`：`Z42_JIT_THRESHOLD` 默认 `unwrap_or(1000)`→`unwrap_or(1)`
- [x] 1.2 `jit/mod.rs` 决策注释更新（为何降默认 + z42c 实证 + OSR 独立处理热循环 + N≥2 不能帮 once-called）
- [x] 1.3 `jit/frame.rs` 过时注释「default 2」→「default 1」订正
- [x] 1.4 `cargo build --release` 无错误

## 阶段 2: 验证
- [x] 2.1 默认（无 env）编 z42c.semantics ~12% 提速（34.7→30.5s）+ **byte-identical** vs baseline
- [ ] 2.2 `cargo test`（全 targets）全绿——尤其 `jit/lazy_tests.rs`（显式 threshold 测试须仍通过）
- [ ] 2.3 **bench 门无回归**：热循环场景（fibonacci/math_loop/arith_loop/polymorphic_dispatch）两 vm 对比——
      本地 micro 场景 sub-50ms 受进程启动主导、比值在噪声内（无显著回归）；权威门 = CI 同-runner A/B（`xtask bench --ab`）。
- [ ] 2.4 完整 GREEN：`xtask test`（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）+ **自举 gen1==gen2 byte-identical**（runtime 改动核心正确性门）
- [ ] 2.5 文档同步：本 proposal + 归档；`jit/mod.rs`/`frame.rs` 注释已随实现更新；roadmap 若索引 JIT tiering 则刷新

## 备注
- **回归风险面**：threshold=1 对「many-cold-functions」程序增加惰性编译开销（但只编**到达**的函数、Cranelift 快、<1% 实测）。热循环程序两 vm 都编热函数 → 无差别。bench 门兜底。
- byte-identical 是硬门：JIT 语义透明，任何产物漂移 = 缺陷。自举 gen1==gen2 守。
- 测量/环境配方见 [[compiler-parallel-heavy-phases-investigation]]。
