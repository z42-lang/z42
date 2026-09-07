# Design: enum 成为独立类型

## Architecture

改动集中在**类型系统的两端**——enum 值从哪来、enum 类型名解析成什么——外加把两端连起来的转换边。
**运行期表示不变**（仍 i64），codegen 只需保证 enum 静态类型的值照旧发 i64。

```
声明      enum Color { Red, Blue }
            │
            ├─ SymbolCollector → SymbolTable.EnumTypes["Color"] + EnumConsts["Color.Red"]=0
            │
   ┌────────┴─────────────────────────────────────────────┐
   │ ① 值的来源                        ② 类型名的解析        │
   │ MemberResolver `Color.Blue`      SymbolTable          │
   │   今天: BoundLitInt(long)          今天: 孤立 Z42ClassType│
   │   之后: BoundLitInt(Color)         之后: enum-aware 类型  │
   └────────┬─────────────────────────────────┬────────────┘
            │                                 │
            └────────► Conversion ◄───────────┘
                 新增 ConvKind.ExplicitEnum
                 （enum ↔ 底层整数，**不进** ImplicitOk）
                        │
                        ├─ BinaryTypeTable：enum==enum / enum 关系比较
                        ├─ PatternBinder：enum 模式 + 关系模式
                        └─ ExprEmitter：enum 静态类型仍发 i64
```

## Decisions

### D1: enum 类型如何表示

**问题**：`Color` 在类型位置今天是 `new Z42ClassType("Color", false, "")`——一个没有基类、没有成员、
和任何东西都不连通的孤立类。它需要携带"我是 enum，底层是 i64"这个事实。

**选项**：
- A — 新增 `Z42EnumType : Z42Type`（带 `UnderlyingType`）。语义最清楚，但要在所有 `is Z42ClassType`
  的分支上补一条（`Conversion` / `MemberResolver` / `StructLayout` / codegen / `OverloadResolver.TypeKey`…）
  ——半径大，且**签名键会变**（`TypeKey` 走 `Name()`，名不变则键不变，但分支漏改就静默降级）。
- B — 沿用 `Z42ClassType`，加 `IsEnum` 标记（~~+ `EnumUnderlying`~~ —— **不需要**：`Type.z42:104`
  明写「z42 一律以 i64 背书 enum」，无 per-enum 底层类型）。所有既有 `is Z42ClassType` 分支自动
  继续工作（enum 仍是 class 家族），只在需要区分的少数点查标记。

**决定：B**。理由：z42 的 enum 运行期就是整数、没有成员方法，不需要独立的类型种类；
用标记能让**改动集中在转换格与运算符表**，而不是散到每个类型分支。
`Z42ClassType` 已有 `_canon`/`_code` 惰性记忆的先例，加标记与既有写法同构。

> 🔴 **必须实证**：`OverloadResolver.TypeKey` 走 `Canon(t.Name())`，enum 名不变 ⇒ 签名键**预期不变**。
> 但这条只能靠自举字节对账确认，不得靠推理（tasks 阶段 4）。

### D2: enum ↔ 整数 —— 双向显式（Q1 建议）

新增 `ConvKind.ExplicitEnum`：**不进** `ImplicitOk()` 白名单、**进** `Exists()`。于是隐式上下文
（赋值 / return / 传参）落 `CheckImplicitConvert` 的 `r.Exists()` 分支报 **E0439**
（"an explicit conversion exists (are you missing a cast?)"）——正是用户需要的指引，而不是
E0402「无转换」。显式 `(long)c` / `(Color)n` 走 `TypeOpTyper` 的 cast 路径放行。

对齐 C#：双向都要 cast。**唯一例外**：常量 `0`（C# 允许 `Color c = 0`）——本变更**先不做**这个例外，
待 Q1 裁决；不做的话 `default` 场景写 `(Color)0`。

### D3: enum 的运算符

- `enum == enum` / `!=`：同类型放行（`BinaryTypeTable` 加 enum 对）。
- `enum < / <= / > / >=`：**放行**（C# 允许），`examples/patterns.z42:65` 的关系模式依赖它。
- `enum == 整数`：**不放行**（需 cast）——这会让 `src/tests/types/enum.z42:43-44` 变红，须改写。
- enum 算术（`+`/`-`）：**不做**（C# 有 `enum ± 底层整数`，z42 暂无用例）。

### D4: 运行期表示不变 —— codegen 的约束

enum 静态类型的值必须继续发 **i64**：`ExprEmitter` 遇到 enum 静态类型时按 scalar/i64 处理，
装箱按 i64 装箱，`GetType()` 折叠沿用既有 `EnumTypeName` 机制（此时类型已知，可简化）。

> ⚠️ **本设计最大的未知在这里**。`PrimModel.IsScalarValue` / `StructLayout` / `BoxIfNeeded` /
> 跨包 TSIG 还原 各自怎么看待"带 IsEnum 标记的 Z42ClassType"，须在 tasks 阶段 1 先**实测摸清**再动手，
> 不得先写实现再验。

## Implementation Notes

- `MemberResolver` 的 enum 分支已把来源 enum 记在 `BoundLitInt.EnumTypeName`——**改类型即可复用该字段**
  定位 enum 类型，不需要新元数据。
- `add-argument-type-check` 在 `OverloadBinder._checkOneArg` 留了 `_isEnumSide` 跳过 + 注释指向本变更；
  **本变更必须摘掉它**（tasks 阶段 3），否则 enum 位永远不检查。
- 跨包：`GCHandleType` 在 z42.core、被 z42c 用。须确认 `ImportedSymbolLoader` 今天怎么还原 enum
  类型名（Q3）——若也退化成普通 `Z42ClassType`，那是与 R1/R3/R5 同族的 imported 保真度缺口，
  需一并补（**先量再决定**）。

## Testing Strategy

- **负例**：`Color c = 0;`（无 cast）→ E0439；`long n = Color.Red;` → E0439；
  `TakeC(0)` → E0439；`Color.Red == 0` → E0439。
- **正例**：`Color c = Color.Blue;` ✅（**今天就编不过，是本变更的核心兑现**）；
  `TakeC(Color.Blue)` ✅；`(long)Color.Red == 0` ✅；`(Color)0` ✅；
  `c switch { Color.Red => …, Color.Blue => … }` ✅（穷尽性不回归）；
  `>= HttpStatus.BadRequest and < HttpStatus.ServerError` ✅（关系模式）。
- **运行期**：golden 用例验 enum 值仍是正确整数（表示不变）。
- 🔴 **反向自检**：把改动整体退回，新增负例必须全红（否则是空门）。
- 🔴 断言用 `DumpBody` / `collectDiags`，**不得只用 `SemanticDump.FirstErrorCode`**
  （它不合并 collector 诊断，签名位置的诊断恒不可见 → 空门）。
- **GREEN** + **自举字节不动点**（D1 的签名键实证）+ `xtask test stdlib --mode jit`
  （本地 `xtask test` 只跑 interp，enum 表示变化须验 JIT 面）。

## Deferred / Future Work

### make-enum-distinct-type-future-flags-and-underlying

- **来源**：本 design Out of Scope
- **触发原因**：`[Flags]` 位运算语义、`enum E : byte` 显式底层类型语法，都不是解开当前不自洽所必需。
- **前置依赖**：本变更
- **触发条件**：出现真实需要位标志 enum 或非 i64 底层宽度的用例时

### make-enum-distinct-type-future-enum-tostring

- **来源**：本 design Out of Scope
- **触发原因**：~~名字反射表需要在元数据里存成员名~~ —— **已存在**（`Enum.GetName`/`GetNames`，
  元数据在 TYPE 记录尾部）。真正缺的只是**把它接到 `ToString()`**：`Color.Red.ToString()` 今天
  返回序号而非成员名（roadmap 另有一条 `crosspkg-enum-tostring` 记同一现象的跨包侧）。
- **前置依赖**：本变更（enum 成为独立类型后，`ToString()` 才有稳定的类型可派发）
- **触发条件**：需要打印 enum 名（日志 / 序列化）时
