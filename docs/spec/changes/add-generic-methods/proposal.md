# Proposal: 泛型方法端到端（方法级 type_args 运行期通道，M1）

> 状态：🔴 DRAFT，待 User 确认 Why / What / Scope。
> 归属：**完整泛型 serde 程序 M1**（M1 方法级 type_args 通道 → M2 serde 引擎 → M3 C# 参照特性面）。
> 类型：**lang/ir/vm**（新调用语法 + 新 IR 指令 + zbc 格式 bump + VM 执行语义）→ 完整流程。

## Why

roadmap 0.5.x 招牌 `JsonSerializer.Deserialize<T>(json)` 是**泛型方法**。当前 z42 **泛型方法端到端不可用**：
- 方法级泛型**声明** `R Foo<T>(...)` 的 `TypeParams` 已能解析（`Decl.z42` / Parser）；
- 但**调用侧断链**——`CallExpr` 无 type-args 字段、parser 不解析调用点 `Foo<MyType>(args)`、方法体内方法级类型形参 `typeof(T)` / `new T()` / `default(T)` 无运行期解析通道。
- 佐证：全 stdlib / 编译器**零**泛型方法用例；`Assert.Throws<E>` / `RunAll<MyTests>` 均被标注"等反射能力增强"而搁置。

**根因**：类型形参的具化信息（type_args）在运行期由**执行上下文**携带——类级由**实例** `Object.instance.type_args` 携带（`exec_address.rs` 的 `default_of` 读 `regs[0]`），而**方法级无载体**：静态泛型方法不是实例，`typeof(T)` 只发裸名 `"T"`、运行期无处解析。

**这是 `Deserialize<T>` 唯一的硬前置**：serde 引擎核心可非泛型（`Deserialize(json, Type)`），但用户可感知的 `<T>` 招牌必须让方法体拿到 `typeof(T)`。

## What Changes

补齐"泛型方法"从**调用语法 → 绑定 → IR → 运行期解析**的完整链路，采用**本质方案（Fork A）**：方法级 type_args 由**执行帧**携带，与类级"实例携带"对称，同属 reified-erasure 模型。

1. **调用语法**：parser 支持调用点显式类型实参 `Foo<T1,T2>(args)`（`<` 与小于号歧义消解）。
2. **AST / 绑定**：`CallExpr` 携带类型实参；`BoundCall` 携带解析后的方法 type_args；TypeChecker 做 arity/约束校验、解析方法体内方法级 `Z42GenericParamType`。
3. **IR + 格式**：Call 指令携带方法 type_arg 名；方法级 `typeof(T)` / `new T()` / `default(T)` 发新指令（或给现有 `TypeofInstr` / `DefaultOfInstr` 加"方法级 scope"判别）→ **zbc/zpkg 格式 bump**（strict-pin；bump 由 ci-bootstrap 两代自举吸收，见 bootstrap-seed.md）。
4. **运行期（interp + jit）**：`Frame` 新增 `method_type_args` 槽，建帧（`exec_call`）时由调用点静态已知实参填入；新址指令按方法级 index 读该槽（镜像类级 `default_of` 读 `regs[0].type_args`）。
5. **support 先行纪律**：本变更只落**支持**（编译器能编、VM 能跑泛型方法）；z42c / stdlib / xtask 源码**本变更内不使用**泛型方法——`Deserialize<T>`（M2）在本支持随下一 nightly 发布后再落地。彻底满足 bootstrap-seed.md「support 先行、晚一 nightly 再 use」。

**边界（M1 只做直接调用）**：反射式 `MethodInfo.MakeGenericMethod().Invoke()`（design.md ③ 完整形态）**不在 M1**——Fork A 的 frame 槽为其铺路，但填充路径不同，拆为独立后续。

## Scope（允许改动的文件）

### 编译器前端（z42c）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | `CallExpr` 加类型实参字段（`TypeExpr[] TypeArgs` + count）|
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | 调用点 `<...>` 类型实参解析 + `<` 歧义消解 |
| `src/compiler/z42c.semantics/src/Bound.z42` | MODIFY | `BoundCall` 加解析后方法 type_args（名数组）|
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | 绑定方法 type_args → 泛型方法 decl；arity/约束校验；方法级 `Z42GenericParamType` 解析 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | call 发方法 type_args；方法级 `typeof(T)`/`new T()`/`default(T)` 发新指令 |

### IR + 二进制格式（`z42.ir` stdlib）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/libraries/z42.ir/src/IrInstr.z42` | MODIFY | Call 携带方法 type_args；方法级形参解析指令（新增或加 scope 判别）|
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 编码新字段/指令 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | 解码 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcVersion.z42` | MODIFY | zbc `Minor` bump |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | zpkg `Minor` bump（内嵌 zbc 版本随动）|

### 运行期（Rust VM）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/interp/mod.rs` | MODIFY | `Frame` 加 `method_type_args` 槽 |
| `src/runtime/src/interp/exec_call.rs` | MODIFY | 建帧时填 `method_type_args` |
| `src/runtime/src/interp/exec_instr.rs` | MODIFY | call 分派传方法 type_args；新指令分派 |
| `src/runtime/src/interp/exec_address.rs` | MODIFY | 方法级形参解析（镜像 `default_of`）|
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 解码 Call 新字段 + 新指令 |
| `src/runtime/src/metadata/types.rs` | MODIFY | 按名解析方法级 type_arg → 具体类型（如需）|
| `src/runtime/src/jit/translate.rs` | MODIFY | jit 镜像：call 传方法 type_args + 形参解析 |
| `src/runtime/src/jit/frame.rs` | MODIFY | jit frame `method_type_args` |

### 测试 + 文档

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/tests/e2e/generic-methods/` | NEW | golden：`typeof(T)` / `new T()` / `default(T)` / 直接调用 + 边界（arity 错、约束违背）|
| `docs/book/src/lang/generics.md` | MODIFY | 方法级泛型章节：机制 + frame 携带 + 与类级对称 |
| `docs/roadmap.md` | MODIFY | 标注 M1 进度（G2 直接调用部分）|
| `src/libraries/z42.ir/README.md` | MODIFY | 功能索引 + 格式版本 |
| `src/runtime/src/interp/README.md` | MODIFY | Frame / 方法级 type_args 说明 |

**只读引用**：
- `src/runtime/src/interp/exec_address.rs` 的 `default_of`（类级镜像范式）
- `src/runtime/src/interp/exec_object.rs`（`obj.new` 填 `instance.type_args` 范式）
- `.claude/rules/version-bumping.md`（格式 bump checklist）
- `.claude/rules/bootstrap-seed.md`（support 先行纪律）
- `docs/spec/changes/plan-generic-reflection/design.md`（G2 ③ 落点参考）

## Out of Scope

- **反射式 `MakeGenericMethod().Invoke()`**（design.md ③ 完整形态）——独立后续。
- **方法 type_args 类型推断**（`Foo(x)` 从实参推 `T`）——M1 **要求显式** `Foo<T>(x)`；推断拆后续。
- **serde 引擎 / `Deserialize<T>`**（M2）——本变更只落"支持"，不写 serde、不在源码使用泛型方法。
- **泛型方法作为委托 / 方法组**（`Func<...> f = Foo<int>`）——后续评估。

## Open Questions

- [ ] **Q1 指令形态**：Call 携带方法 type_args 是"扩 Call 指令加字段" vs "紧邻 Call 前发一条 `SetMethodTypeArgs` 指令"？前者省一条指令、后者对 Call 编码侵入小。→ design.md 定。
- [ ] **Q2 方法级 vs 类级形参判别**：`typeof(T)`/`default(T)` 现按 index 读 `regs[0]`（类级）。方法级如何判别？"新指令" vs "现指令加 1bit scope 标志"？→ design.md 定。
- [ ] **Q3 类级 `typeof(T)` 现运行期解析路径**：需确认类级 `typeof(T)` 当前如何把裸名 `"T"` 解析成实例具体类型（`TypeofInsn` 携 `type_args`），方法级复用还是新建。→ design.md 勘察确认。
- [ ] **Q4 `default(T)` 是否纳入 M1**：serde 只需 `typeof(T)`；`new T()`/`default(T)` 是完整性。三者共用同一 frame 槽，边际成本低 → 倾向全纳入。请 User 确认。
