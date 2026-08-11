# Proposal: 收紧隐式转换 + 插 ConvertInstr + 迁移（tighten-implicit-conversions）

> 三 PR 阶梯之 **PR2 / 3**。总纲：C# 风格隐式/显式转换体系（比 C# 更严更可预测）。
> PR1（已合并 `d194bf5b`）立分类器基础设施 → **PR2（本变更）收紧执行门 + 为数值转换插
> `ConvertInstr` + 迁移调用点** → PR3 加用户自定义 `implicit`/`explicit operator`。

## Why

PR1 把转换判定集中进 `Conversion.Classify`，给每种转换打了**语义正确**的种类标签
（窄化 / 有损浮点 → `ExplicitNumeric`），但执行门 `ImplicitOkPermissive()` **临时放行**
`ExplicitNumeric`，使产物与历史逐字节等价。于是今天仍有三类**已实测的**静默 bug：

| 语句 | 当前（PR1）| 应当 |
|------|-----------|------|
| `byte b = 300;` | **300**（不截断）| 编译错误（越界常量）→ `(byte)300` |
| `int nb = someLong;`（值 5e9）| **5000000000**（不截断）| 编译错误（非常量窄化）→ `(int)` |
| `double d = 5;` | `5`（运行期是 I64，不是 F64）| `5`（真 F64）——隐式拓宽应插 `ConvertInstr` |
| `(byte)300` | 44 ✓ | 44 ✓（显式 cast 已正确截断）|

根因两条：① 门放行窄化 → 该报错的不报错；② **隐式数值转换点不发 `ConvertInstr`**——所有整数
运行期同为 `Value::I64`，隐式拓宽 `int→double`（I64→F64）从不真转、隐式窄化从不截断（只有显式
`(T)x` 才走 `_emitConvert` 发 `ConvertInstr`）。PR2 同时修掉这两条。

## What Changes

1. **收紧执行门**（`Conversion.z42`）：`ImplicitOkPermissive()` → `ImplicitOk()`，白名单剔除
   `ExplicitNumeric`（`Unboxing` / `ExplicitRef` 本就不在）。窄化 / 有损浮点在隐式上下文不再放行。

2. **C# 常量在范围内隐式例外**（User 裁决 2026-08-11）：窄化目标是**整数** prim 且源是**编译期
   常量整数**、其值落在目标范围内 → **仍隐式放行**（`byte b = 48;` 合法，`byte b = 300;` 报错）。
   与「隐式只允许绝对无损」一致——在范围内的常量窄化是**逐值可证无损**的。有损浮点（`float f = 5;`）
   **不含**此例外（决策 3：有损浮点一律显式）。

3. **`ConvertIfNeeded` 插入 `ConvertInstr`**（`TypeChecker.z42`，镜像 `BoxIfNeeded`）：在隐式数值
   转换点（return / var-decl / assign / call-arg）当**运行期表示类**变化（int↔float、char→数值）时
   包 `BoundConvert` → `_emitConvert` 发 `ConvertInstr`。整数等宽拓宽（byte→int→long，运行期同 I64）
   与 f32→f64（运行期同 F64）是 no-op，不插 → 最小化字节扰动。

4. **新诊断 E0439**：`cannot implicitly convert 'X' to 'Y'; an explicit conversion exists (are you
   missing a cast?)`——在存在**显式**转换却用于隐式上下文时报（与「根本不存在转换」的 E0402 区分）。

5. **迁移**：stdlib + z42c 源里**真正**的窄化点（越界常量 / 非常量窄化 / 有损浮点隐式）补 `(T)` cast。
   在范围内的常量窄化（binary-format writer 里 `bytes[i] = 48;` 这类）因例外 2 **无需改动**。

6. 负向单测（窄化拒绝 / 常量例外接受）+ 截断正确性 e2e + 机制文档更新。

## 破坏性 / 自举纪律

- **破坏性**：拒绝今天接受的程序（非常量 / 越界窄化、有损浮点隐式）。这是**有意的**收紧。
- **产物字节会变**（多了 `ConvertInstr` + 迁移改源）→ 不再是 PR1 的字节不动点；验证改为**逐 golden
  对齐 + 自举收敛**（gen_n == gen_{n+1}，与 PR1 的字节不同）。z42c 自身 codegen 变（`ConvertIfNeeded`
  影响 z42c 编出的产物）→ 预期**破一代**，warm 重建自愈至 5/5（见 [[opt-pipeline-passes]] 同款 D7）。
- **自举链不断**：迁移只**添加** `(T)` cast（合法旧语法），上一 nightly 的 z42c 照样能编迁移后的源；
  收紧是**新 z42c 的行为**，与旧 nightly 能否编当前源正交。**先迁移源、再收紧门**（否则 z42c 自编不过）。
  不引入新语法 / 不 bump 格式（`ConvertInstr` opcode 0xB1 已存在）。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.semantics/src/Conversion.z42` | MODIFY | `ImplicitOkPermissive`→`ImplicitOk`（剔除 ExplicitNumeric）|
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | `ConvertIfNeeded` + `CheckImplicitConvert`（分类 + 常量例外 + E0439/E0402 分流）+ 常量范围判定 |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | return / var-decl：改用 `CheckImplicitConvert` + `ConvertIfNeeded` |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | assign / array-store：同上 |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | call-arg / params 元素：同上 |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新增 `ImplicitNarrowingConversion = "E0439"` |
| `src/libraries/**/*.z42` | MODIFY | 迁移真窄化点补 `(T)` cast（binary writer 为主；exact 清单由 IMPL grind 产出）|
| `src/compiler/z42c.*/src/**/*.z42` | MODIFY | 迁移 z42c 源自身真窄化点（若有）|
| `src/compiler/z42c.semantics/tests/conversion/*.z42` | MODIFY/NEW | 负向（窄化拒绝）+ 常量例外接受 + 截断单测 |
| `examples/` 或 e2e | NEW | 截断正确性 e2e（`(byte)300==44`、`byte b=48` OK、越界报错）|
| `docs/book/src/compiler/type-conversion.md` | MODIFY | 收紧门 + 常量例外 + ConvertInstr 插入机制 |
| `.claude/projects/.../memory/type-conversion-system-program.md` | MODIFY | 归档后更新续推口令状态到 PR3 |

## Out of Scope

- **用户自定义转换**（`implicit`/`explicit operator`、`(C)x` 语法扩展、`as`/`is` 接入）→ PR3。
- 常量**表达式**折叠范围：PR2 常量例外覆盖**字面量常量**（`BoundLitInt` / 负号字面量 / const 折叠到字面量）；
  任意常量表达式（`byte b = 40 + 8;`）的完整折叠若超出现有 `ConstFold` 覆盖面 → 可留 PR2 follow-up
  （不阻塞主线；IMPL 时按实测 stdlib 需求定）。
- 运行期 `convert_value` 改动（已正确，PR1 阶段已验 `(byte)300==44`）。
- 格式 bump / 新 IR 指令（`ConvertInstr` / `AsCast` 已存在）。

## 验证

- `xtask test` 全绿（含新负向 / 常量例外 / 截断单测）。
- 自举收敛：`xtask test` 的自举 gen_n == gen_{n+1}（与 PR1 字节不同；破一代 warm 重建自愈）。
- e2e：`byte b=48`→48、`byte b=300`→E0439、`int nb=long`→E0439、`(byte)300`→44、`double d=5`→真 F64。
- `xtask test bootstrap`：上一 nightly z42c 仍能编迁移后的源（无语法/格式越界）。
