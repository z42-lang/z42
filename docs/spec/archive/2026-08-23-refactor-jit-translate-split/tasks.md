# Tasks: refactor-jit-translate-split

> 状态：🟢 已完成 | 完成：2026-08-23
> 类型：refactor（零行为变更）+ 新增差分测试
> **变更说明**：translate.rs 拆 12 子模块 + interp/JIT 语义三重实现收敛到 semantics.rs
> **原因**：文件超限 2983>500；语义三处漂移隐患无测试防护
> **文档影响**：jit/interp README、book vm-architecture、runtime_review 跟踪表

## 进度概览
- [x] 阶段 1: H3 语义收敛（commit 1）✅
- [x] 阶段 2: H2 translate 拆分 ✅
- [x] 阶段 3: 验证与文档 ✅

## 阶段 1: H3 语义收敛（commit 1，不动文件拆分）
- [x] 1.1 新建 `semantics.rs`：迁 `int_binop`/`int_bitop`/`numeric_lt`/`eval_cmp`（从 interp/ops.rs 抽标量核，寄存器包装留 ops.rs）
- [x] 1.2 `semantics.rs` 迁 `convert_value`/`convert_from_{f64,i64,char}`（从 exec_value.rs）+ `div_by_zero_check`
- [x] 1.3 `semantics.rs` 载重决策具名常量/枚举（OVERFLOW/INT_DIV_ZERO/INT_DIV_GUARD/F2I/I2F/CMP）
- [x] 1.4 `lib.rs` 挂 `mod semantics;`
- [x] 1.5 interp/ops.rs + exec_value.rs 改调 `semantics::*`（删本地重复逻辑）
- [x] 1.6 jit/helpers/mod.rs 删 `int_binop_helper`/`numeric_lt_helper`，改调 `semantics::*`
- [x] 1.7 jit/helpers/arith.rs 删除零重复 + i64 快路径引 semantics 常量；object.rs `jit_convert` 委托改 semantics
- [x] 1.8 translate.rs 的 `emit_*` 加 `// SEMANTICS: semantics::<fn>` 锚注释（拆分前先锚）
- [x] 1.9 收敛不支持指令清单为 `unsupported_reason(&Instruction)`（暂放 translate.rs，拆分时移 unsupported.rs）
- [x] 1.10 新增 `semantics_tests.rs`（标量规则边界单测）
- [x] 1.11 新增差分测试骨架：标量规则 vs JIT 编译执行，覆盖 MIN/-1、/0、%0、NaN 比较、f64 饱和、T_U64、shift≥64
- [x] 1.12 `cargo test --lib` 全绿 → commit 1

## 阶段 2: H2 translate 拆分（commit 2a leaf + 2b 本体）
- [x] 2a 先抽 8 个 leaf 子模块（emit_int/emit_fc/predicates/analysis/reg_var/ic/control/unsupported）；translate.rs→translate/mod.rs（3036→1948）→ **commit 804f0fd8**
- [x] 2.1 引入 `TxCtx<'a,'b>`（ctx.rs），68 个 helper FuncRef 内联为字段（与 `let hr_x` 局部同名 → 构造用 field shorthand），弃 HelperRefs 中间结构
- [x] 2.2 6 个函数内 macro → `impl TxCtx` 方法（ri/str_val/regs_val/check/dispatch_to_catch_or_return/emit_int_divrem）
- [x] 2.3 `mod.rs` 驱动：逐块构造 `TxCtx` + 按类别穷尽 dispatch（无 `_` 兜底，不支持指令内联 bail）+ `tr_terminator`
- [x] 2.4/2.5 指令臂下沉 `impl TxCtx { fn tr_* }`：value/arith/compare/convert/call/array/object/structs/term；循环不变量提升 → hoist.rs
- [x] 2.6 可见性 `pub(super)`；旧 translate.rs 经 `git mv` 成 translate/mod.rs（无需改 jit/mod.rs 的 `mod translate;`）
- [x] 2.7 差分担保沿用阶段 1 的 `arith_semantics_edge` golden（interp+jit）+ 现有 JIT golden 套件；未另建 Rust 级 diff（JIT-run harness 成本高，与本仓约定一致——见 design D2）
- [x] 2.8 逐文件 <500 确认：translate/ 全 20 文件 ≤ 493（mod.rs 493，最大 emit_fc 304）
- [x] 2.9 `cargo test --lib` 958/0 + 边界 golden interp+jit 2/0 → **commit 017f4f85**

## 阶段 3: 验证与文档
- [x] 3.1 `cargo build --release`（z42vm）无错
- [x] 3.2 `xtask test` 完整 GREEN：**✅ all stages passed（C#-free）**——e2e / cross-zpkg / stdlib /
      **compiler 自举 5/5 gen1==gen2 byte-identical** / vscode-syntax
- [x] 3.3 JIT 逐字节对 interp：边界 golden `arith_semantics_edge` 在 e2e 的 interp+jit 双模式各 1/0
- [x] 3.4 `cargo test --lib` 958/0（含 jit lazy/vm_interface + semantics 35 例）
- [x] 3.5 文档同步：jit/README（translate/ 20 子模块）+ interp/README（ops.rs 去 numeric_lt）+
      **book 新页 `runtime/interp-jit-semantics.md`**（H3 单一真相源机制）+ SUMMARY 挂入
- [x] 3.6 runtime_review.md 跟踪表 #4 标 🟢；H4 裁决**却下**并落 review 段
- [x] 3.7 归档 → `docs/spec/archive/2026-08-23-refactor-jit-translate-split/` + 开 PR

## 备注
- 先 H3 后 H2：emit_* 先锚定，拆分连锚搬移，避免回锚返工。
- H4（write barrier）已裁决**却下**（helper 已安全 no-op，与既有测试契约冲突）——本变更仅在 runtime_review 跟踪表标记，不改代码。
