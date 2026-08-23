# Proposal: refactor-jit-translate-split

## Why
`jit/translate.rs` 2983 行（硬限 500），单个 `translate_function` 1770 行——超限严重、难维护。
同时算术/比较/转换语义在 interp、JIT helper、JIT 内联 Cranelift **三处各写一遍**，靠手写注释维持一致，
历史多次漂移（`fix-jit-int-div-by-zero`、`fix-char-comparison`）。对「编译器全自举 byte-identical」目标，
interp↔JIT 语义分歧是致命隐患。不做：文件持续膨胀 + 语义漂移风险无静态/测试防护。

## What Changes
- 新建 crate 顶层 `semantics.rs` 作标量规则唯一真相源；interp + JIT helper 都调它，删除逐字重复。
- JIT 内联 `emit_*` 加 `// SEMANTICS:` 锚注释 + 新增**差分测试**把注释担保升级为测试担保。
- 两处不支持指令清单收敛为单个 `unsupported_reason(&Instruction)`。
- `translate.rs` 拆为 `jit/translate/` 12 子模块（引入 `TxCtx` 上下文结构），每文件 <500 行。
- **零外部行为变更**（纯 refactor + 新增测试）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/semantics.rs` | NEW | 标量规则唯一真相源 + 载重决策具名常量 |
| `src/runtime/src/semantics_tests.rs` | NEW | semantics.rs 单测（标量规则边界） |
| `src/runtime/src/lib.rs` | MODIFY | 挂 `mod semantics;` |
| `src/runtime/src/interp/ops.rs` | MODIFY | `int_binop`/`numeric_lt`/`eval_cmp` 改为寄存器取值后调 `semantics::*` |
| `src/runtime/src/interp/exec_value.rs` | MODIFY | `convert_value`/`check_int_div_by_zero` 迁至 semantics，本文件改调 |
| `src/runtime/src/jit/helpers/mod.rs` | MODIFY | 删 `int_binop_helper`/`numeric_lt_helper` 重复，改调 `semantics::*` |
| `src/runtime/src/jit/helpers/arith.rs` | MODIFY | 删除零重复，快路径 i64 规则改引 semantics 常量；调用点适配 |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | `jit_convert` 委托目标改 `semantics::convert_value` |
| `src/runtime/src/jit/translate.rs` | DELETE | 拆分后删除（内容迁至 `translate/`） |
| `src/runtime/src/jit/translate/mod.rs` | NEW | 薄驱动：prescan + dispatch/terminator 路由 |
| `src/runtime/src/jit/translate/ctx.rs` | NEW | `TxCtx` + 宏转方法 + 寄存器访问 |
| `src/runtime/src/jit/translate/arith.rs` | NEW | 整型/bool 算术臂 + 谓词 |
| `src/runtime/src/jit/translate/arith_emit.rs` | NEW | `emit_i64_*`/`emit_bool_*`/`emit_f64_binop/neg`（H3 锚定面） |
| `src/runtime/src/jit/translate/compare.rs` | NEW | 比较臂 + `emit_i64_cmp`/`emit_f64_cmp` |
| `src/runtime/src/jit/translate/convert.rs` | NEW | Convert 臂 + `emit_*_convert` |
| `src/runtime/src/jit/translate/value.rs` | NEW | Const/Copy/StrConcat/ToStr 臂 + `emit_const_*` |
| `src/runtime/src/jit/translate/call.rs` | NEW | Call/Builtin/CallIndirect/闭包臂 + IC 辅助 |
| `src/runtime/src/jit/translate/array.rs` | NEW | 数组臂 + 元素谓词 |
| `src/runtime/src/jit/translate/object.rs` | NEW | 对象/字段/VCall/类型检查臂 + IC 辅助 |
| `src/runtime/src/jit/translate/structs.rs` | NEW | struct 值类型指令臂 |
| `src/runtime/src/jit/translate/control.rs` | NEW | terminator 臂 + safepoint + catch 分发 |
| `src/runtime/src/jit/translate/unsupported.rs` | NEW | `unsupported_reason(&Instruction)` 单表 |
| `src/runtime/src/jit/translate/semantics_jit_diff_tests.rs` | NEW | 差分测试：标量规则 vs JIT 编译执行 byte-identical |
| `src/runtime/src/jit/mod.rs` | MODIFY | `mod translate;` 指向新目录；`HelperIds` 重导出适配 |
| `src/runtime/src/jit/README.md` | MODIFY | 核心文件表：translate.rs → translate/ 子模块；补 semantics 依赖 |
| `src/runtime/src/interp/README.md` | MODIFY | 补 ops.rs/exec_value 依赖 semantics 的说明 |
| `src/runtime/src/README.md` | MODIFY | 若存在：登记新增 semantics 模块（否则新建目录 README 由 code-org 判定） |
| `docs/book/src/runtime/vm-architecture.md` | MODIFY | H3 语义单一真相源机制 + JIT/interp 收敛策略落知识库 |
| `docs/runtime_review.md` | MODIFY | 跟踪表 #4 标记完成；H4 按裁决标记却下 |

**只读引用**：
- `src/runtime/src/interp/exec_instr.rs` — dispatch 组织样板参照
- `src/runtime/src/jit/helpers/registry.rs` — `HelperIds` 结构复用
- `src/runtime/src/jit/frame.rs` / `reg_access.rs` — TxCtx 依赖的寄存器/帧访问
- `src/runtime/src/metadata/superinstr.rs` — `CmpOp` 定义

## Out of Scope
- 不改任何算术/比较/转换的**行为**（wrapping / 除零抛出 / 饱和 / NaN 序 一律保持现状）。
- 不动 M3 opcode 常量表、M4 zbc_compat（属 #3 change）；不动 vm_context/reflection 等其他 god 文件。
- 不引入 computed-goto / dispatch 优化（属 interp perf 线）。

## Open Questions
- [ ] `semantics.rs` 是否顺带把 `interp/ops.rs` 的 `eval_cmp_i64`（typed I64 快路径）也纳入真相源？
      倾向：纳入其"标量规则"注释锚，但函数保留在 interp（它是 interp 执行模型专属的 unchecked 取值）。
