# Tasks: refactor-jit-translate-split

> 状态：🟡 进行中 | 创建：2026-08-23
> 类型：refactor（零行为变更）+ 新增差分测试
> **变更说明**：translate.rs 拆 12 子模块 + interp/JIT 语义三重实现收敛到 semantics.rs
> **原因**：文件超限 2983>500；语义三处漂移隐患无测试防护
> **文档影响**：jit/interp README、book vm-architecture、runtime_review 跟踪表

## 进度概览
- [x] 阶段 1: H3 语义收敛（commit 1）✅
- [ ] 阶段 2: H2 translate 拆分（commit 2）
- [ ] 阶段 3: 验证与文档

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

## 阶段 2: H2 translate 拆分（commit 2，纯机械）
- [ ] 2.1 引入 `TxCtx<'a,'b>` 结构 + `HelperRefs`（复用 helpers/registry HelperIds）
- [ ] 2.2 6 个文件内 macro → `impl TxCtx` 方法（ri/str_val/regs_val/check/dispatch_to_catch/emit_int_divrem）
- [ ] 2.3 建 `translate/` 目录，`mod.rs` 驱动（prescan + 穷尽 dispatch/terminator 路由）
- [ ] 2.4 拆 ctx.rs / arith.rs / arith_emit.rs / compare.rs / convert.rs
- [ ] 2.5 拆 value.rs / call.rs / array.rs / object.rs / structs.rs / control.rs / unsupported.rs
- [ ] 2.6 可见性调整（pub(super)）；删除旧 translate.rs；jit/mod.rs 指向新目录
- [ ] 2.7 差分测试文件归位 `translate/semantics_jit_diff_tests.rs`
- [ ] 2.8 逐文件确认 <500 行、分发函数 <60 行
- [ ] 2.9 `cargo test --lib` 全绿 → commit 2

## 阶段 3: 验证与文档
- [ ] 3.1 `cargo build --release`（z42vm）无错
- [ ] 3.2 `xtask test`（完整 GREEN：e2e / cross-zpkg / stdlib / compiler 自举 gen1==gen2 / vscode-syntax）
- [ ] 3.3 `xtask test e2e --mode jit`（JIT 逐字节对 interp）
- [ ] 3.4 差分测试全绿；`cargo test --lib jit` + `semantics`
- [ ] 3.5 文档同步：jit/README + interp/README + book vm-architecture（H3 单一真相源机制）
- [ ] 3.6 runtime_review.md 跟踪表 #4 标 ✅；H4 按裁决标却下
- [ ] 3.7 归档 + 开 PR

## 备注
- 先 H3 后 H2：emit_* 先锚定，拆分连锚搬移，避免回锚返工。
- H4（write barrier）已裁决**却下**（helper 已安全 no-op，与既有测试契约冲突）——本变更仅在 runtime_review 跟踪表标记，不改代码。
