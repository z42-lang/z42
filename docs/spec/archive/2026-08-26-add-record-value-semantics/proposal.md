# Proposal: 给 `[Record]` 类型加值语义

## Why

`record` 关键字→`[Record]` attribute 的替换已完成（archive `…-replace-record-keyword-with-attribute`，
#295/#296/#298），但那次**只做等价替换、不含值语义**（明确 Deferred）。当前 `[Record] class` 仍拿
`Std.Object` 的身份版 `Equals`/`GetHashCode`（按句柄）与 identity `==`——两个同值实例 `==` 为 `false`，
`ToString()` 只给类型名。这不是「record」应有的语义。C# record 的核心正是 **值相等 + 记录式 ToString**。
本变更补齐这个核心，让 `[Record]` 名副其实。

## What Changes

- **`[Record] class`**（引用类型，真正的缺口）：合成 member-wise `Equals(object)` + `GetHashCode()` +
  `operator ==` / `!=` + 记录式 `ToString()`，**type-exact**（运行时类型不同即不等，对齐 C# EqualityContract）。
- **`[Record] struct`**：值相等已由既有 blob-struct 合成路（`EmitSynthStructEquals`）具备、GetHashCode 由 VM
  原生 FNV 已是值哈希——**仅补记录式 ToString**（此前为类型名，属可观察变更）。
- **VM 让路（struct ToString）**：VM 的 boxed-struct 分派对零参 `ToString` **无条件原生拦截**返回类型名
  （interp `exec_vcall.rs` + JIT `jit/helpers/vcall.rs`），合成方法够不着。加一处 **record-bit 守卫**：record
  struct 跳过原生拦截 → 落到合成的 `<Type>.ToString`。格式化逻辑**统一留在编译器合成**，VM 只让路。
- **字段范围（对齐 C#）**：相等比**全部实例字段**（含 private / 位置字段 / 块内声明）；ToString 只打
  **public 成员**。
- **无新语法、无 zbc/zpkg 格式 bump、无两-nightly**（record bit3 已在格式中；纯合成 + 一处 VM 守卫）。

**Out（Deferred，各自独立 change）**：`with` 表达式（需新语法+两-nightly）、`Deconstruct`（需 tuple）、
`init`-only setters。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/RecordSynth.z42` | NEW | record 值语义合成器：class 的 `Equals$1`/`GetHashCode`/`ToString`、struct 的 `ToString`；type-exact 门 + 逐字段比较/哈希组合 + 记录式串拼接 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 合成循环内为 record 类型（`HandlerRegistry.HasRecord`）接线调用 `RecordSynth`（最小接线；IrGen 已 611 行超限，只加派发不加逻辑） |
| `src/compiler/z42c.semantics/src/OperatorEmitter.z42` | MODIFY | record-class 操作数的 `==`/`!=` → 发 null-safe 值 `Equals` 调用（镜像既有 blob-struct `==` 拦截，OperatorEmitter:29） |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42ClassType` 加 `IsRecord` 标志（供 OperatorEmitter 判定 record-class；实施期发现——`==` 拦截需可查询的语义层 record 位）|
| `src/compiler/z42c.semantics/src/StubCollector.z42` | MODIFY | 从 `HandlerRegistry.HasRecord(raw decl)` 回填 `IsRecord`（镜像既有 `IsDeprecated` 回填，实施期发现）|
| `src/runtime/src/metadata/types.rs` | MODIFY | 加 `is_record()` 访问器（镜像 `is_struct()`:898，读 `CLASS_FLAG_RECORD`） |
| `src/runtime/src/interp/exec_vcall.rs` | MODIFY | boxed-struct 零参 `ToString` 原生拦截加 `!is_record()` 守卫（:216-220） |
| `src/runtime/src/jit/helpers/vcall.rs` | MODIFY | 同守卫（:190-196，JIT 镜像臂） |
| `src/tests/attributes/record_value_semantics.z42` | NEW | e2e 自检（Assert）：class & struct 的 Equals/==/!=/GetHashCode/ToString、type-exact、null、异类型、嵌套引用字段、public/private 字段范围（无 expected 文件——纯 Assert，空 stdout，同 `record_attribute.z42`）|
| `docs/book/src/language/record-attribute.md` | MODIFY | 值相等/ToString 从 Deferred 上移正文；记录 type-exact、字段范围、ToString 格式、struct-ToString VM 守卫机制 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件加 `RecordSynth.z42` |

**只读引用**（理解上下文必须读，不修改）：

- `src/compiler/z42c.semantics/src/FunctionEmitter.z42` — `EmitSynthStructEquals`（合成范式蓝本）
- `src/compiler/z42c.semantics/src/ExprEmitter.z42` / `OperatorEmitter.z42` — `EmitSynthEqualsResult` / `_emitStructEquality` / `_emitLeafEqChecks`（逐字段比较范式）
- `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` — bit3 写入（`hasRecord`）
- `src/compiler/z42c.semantics/src/HandlerRegistry.z42` — `HasRecord()`
- `src/libraries/z42.core/src/Object.z42` — Object 四方法（ToString/Equals/GetHashCode/GetType）
- `src/runtime/src/corelib/convert.rs` / `corelib/object.rs` — 原生 struct hash / Object 默认版
- `src/runtime/src/metadata/bytecode.rs` — `CLASS_FLAG_RECORD`

## Out of Scope

- `with` 表达式、`Deconstruct`、`init`-only setters（Deferred，各自独立 change）
- 任何 zbc/zpkg 格式变更（record bit3 已存在，不 bump）
- IrGen god-class 拆分（611 行超限属既有债，由 compiler-structure-refactor 程序单独处理；本变更只加最小派发）

## Open Questions

- [ ] struct ToString 采 **S1（VM 让路 + 编译器合成）** 还是 S2（VM 原生格式化）？design 推荐 S1（格式化单一真相源在编译器，VM 改动最小）——待 6.5 确认。
- [ ] type-exact 的运行期机制：`other.GetType() == this.GetType()` 身份比较是否可靠（Type 对象是否 per-type 单例）？design 给方案，实施时坐实。
- [ ] 含基类的 record-class：合成 Equals 的字段枚举是否需沿基链上溯收集继承字段？design 给方案。
