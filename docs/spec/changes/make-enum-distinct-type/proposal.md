# Proposal: enum 成为独立类型（对标 C#）

## Why

**z42 现在的 enum 模型自相矛盾：成员是 `long`，类型名是一个谁也转不进去的孤立类。**

两半各自"按设计"，但**转换格里没有边把它们连起来**：

| 现场 | 类型 | 出处 |
|---|---|---|
| `Color.Blue`（成员引用） | `long` | [`MemberResolver.z42:36-46`](../../../../src/compiler/z42c.semantics/src/MemberResolver.z42) 刻意绑成 `BoundLitInt` |
| `Color`（类型位置） | 孤立 `Z42ClassType` | [`SymbolTable.z42:255`](../../../../src/compiler/z42c.semantics/src/SymbolTable.z42) `EnumTypes` 命中 → `new Z42ClassType(n, false, "")` |

后果（实测，`scratch/e`）：

```z42
public enum Color { Red, Blue }
void TakeC(Color c) { }
void TakeL(long v) { }

Color c = Color.Blue;   // ❌ E0402: cannot assign long to Color (var-decl)
long n = Color.Red;     // ✅
TakeC(Color.Blue);      // ❌ E0402: cannot assign long to Color (argument)
TakeL(Color.Red);       // ✅
```

**即：今天无法产生一个 enum 类型的值。** enum 只在"当整数用"时可用，一旦出现在 enum 类型的变量 /
形参 / 字段位置就编不过。这就是欠债表里的 **bug D**。

`Color c = Color.Blue` 的失败**与实参检查无关**——它今天在 var-decl 就报错，只是
[`restore-emit-zbc-diagnostics-program`](../../../../.claude/) 之前 `--emit-zbc` 把诊断吞了，没人看见。

### 为什么现在做

[`add-argument-type-check`](../add-argument-type-check/) 开启实参检查后，`GCHandle.z42` 的
`GCHandle.Alloc(target, GCHandleType.Weak)` 会红——**实参是枚举成员引用本身**。那个 PR 已把 enum 位
**跳过**并登记为残留洞，等本变更定音后摘掉。

> 🔴 **明确排除的做法**：在调用点补 `(GCHandleType)` cast 把红变绿。那是拿 cast 掩盖
> 「enum 成员产不出 enum 类型值」这个 bug，违反本程序铁律。

## What Changes

**enum 成为独立类型**（C# 语义）：

- `E.Member` 的静态类型从 `long` 改为 `E`。
- enum ↔ 底层整数需**显式 cast**（`(long)c` / `(Color)n`）。
- enum 之间的 `==` / `!=` / 关系比较（`<` `>=`）照常；enum 与整数直接比较需 cast。
- 运行期表示**不变**（仍是 i64），只动类型系统与诊断。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | `E.Member` 绑定类型 `long` → enum 类型 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | enum 类型名解析：孤立 `Z42ClassType` → 带底层类型标记 |
| `src/compiler/z42c.semantics/src/Conversion.z42` | MODIFY | 新增 `ConvKind.ExplicitEnum`（enum ↔ 整数，**不进** `ImplicitOk`） |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | enum 类型的 `IsAssignableTo` / scalar 判定 |
| `src/compiler/z42c.semantics/src/BinaryTypeTable.z42` | MODIFY | enum 运算符（`==`/`!=`/关系）定型 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | enum 值发 i64（表示不变） |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | enum 模式 / 关系模式 |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | **摘掉** `add-argument-type-check` 留的 enum 跳过 |
| `src/tests/types/enum.z42` | MODIFY | 4 条断言按新语义改写（见下） |
| `examples/patterns.z42` | MODIFY | 关系模式 `>= HttpStatus.BadRequest` 确认可用 |
| `src/libraries/z42.core/src/GC/GCHandle.z42` | MODIFY | 若新语义下仍需调整 |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | **改写 enum-as-int SoT 段** |
| `docs/book/src/language/enums.md` | NEW | enum 语义专页（现无） |
| `docs/spec/changes/make-enum-distinct-type/**` | NEW | 本变更容器 |

**只读引用**：`src/compiler/z42c.semantics/src/ExhaustCheck.z42`、`ClassDescBuilder.z42`、
`ConstBlob.z42`、`ImportedSymbolLoader.z42`（enum 跨包）。

## 爆炸半径（已实测）

全仓 **8 个 enum 类型**（`Color` / `Direction` / `GCHandleType` / `HttpStatus` / `Palette` /
`RootKind` / `Status` / `TypeVisibility`），成员引用 **79 处**。

| 现场 | 影响 |
|---|---|
| `z42.core/src/Type.z42:160` `public extern TypeVisibility Visibility { get; }` + `Visibility == TypeVisibility.Public`（7 处测试） | ✅ **变自洽**——今天是「enum 类型的属性」比「`long` 的成员」，正是同一处不一致 |
| `src/tests/types/enum.z42:35,36,43,44` `Assert.Equal(404, Status.NotFound)` / `Direction.North == 0` | ❌ **成文断言的正是 enum-as-int**，必须按新语义改写（加 cast） |
| `examples/patterns.z42:65` `>= HttpStatus.BadRequest and < HttpStatus.ServerError` | ⚠️ 关系模式须支持 enum 操作数（C# 允许） |
| `switch` / `c switch { Color.Red => … }`（`ExhaustCheck` 一族） | ⚠️ 两侧同为 enum 类型，预期更简单，需验 |
| codegen / 装箱 / `PrimModel` | ⚠️ **最大未知**：enum 类型值仍须发 i64；装箱、`GetType()` 折叠、跨包 TSIG 都要跟 |

## Out of Scope

- **不改运行期表示**（仍 i64）；不引入 `[Flags]`、不引入显式底层类型语法（`enum E : byte`）。
- **enum 反射面不动**——`Type.GetEnumUnderlyingType()` / `Enum.Parse` / `Enum.IsDefined` 与 zbc 的
  enum 元数据（`Flags` bit5 + 成员名·i64 值）**main 已有**，本变更只动类型系统与转换格，不碰它们。
- **反射名字表也已存在**（`Enum.GetNames` / `GetValues` / `GetName` / `Parse` / `IsDefined`，
  `z42.core/src/Enum.z42`）——起草时误写成"另议"。本变更同样不碰。
- ⭐ **`Type.z42:104` 是明确的 SoT：「z42 一律以 i64（long）背书 enum」**（值存 i64、`GetValues`
  返 `long[]`）⇒ **不存在 per-enum 底层类型** ⇒ design D1 里的 `EnumUnderlying` 字段是多余的，
  只需 `IsEnum`。
- ⚠️ **边界**：反射 API 的签名是 long 本位（`Parse`→`long`、`IsDefined(Type, long)`）。enum 成为
  独立类型后，这些调用点要么加显式 cast、要么反射面维持 long 本位（反射本就无类型）——**须在
  阶段 0 一并定，别等实现到一半才发现**。

## Open Questions

- [ ] **Q1**：enum ↔ 整数是**双向都要 cast**，还是允许 `enum → 整数`隐式（C# 要求双向 cast，
      `0` 字面量除外）？建议**对齐 C#：双向都要**，唯一例外是常量 `0`。
- [ ] **Q2**：`Direction.North == 0` 这类既有断言，改写成 `(long)Direction.North == 0` 还是
      `Direction.North == (Direction)0`？建议前者（意图是"验底层值"）。
- [ ] **Q3**：跨包 enum（`GCHandleType` 在 z42.core、被 z42c 使用）的 TSIG 表示要不要带底层类型？
      需先确认 `ImportedSymbolLoader` 今天怎么还原 enum 类型名。
