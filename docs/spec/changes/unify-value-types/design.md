# Design: unify-value-types Phase 1（编译器核心 —— 消灭 Z42PrimType）

> 基于两次内脏调查（编译器 + runtime，2026-08-12，origin/main e00cf904）。原始架构 DRAFT
> `docs/spec/archive/2026-08-09-add-struct-value-semantics/design-radical.md` 的行号 + R3 前提已过时
> （P4b 把 struct 装箱改成堆 ScriptObject），本文以当前 main 为准。

## Architecture

```
现状（二元）:
  int  ──ResolveTypeP:113──▶ Z42PrimType("int")   ──┐（无元数据，纯名字）
  Point ─────────────────▶ Z42ClassType(IsStruct)──┤ StructLayout(字节布局+引用位图)
                                                     │
  七表投影: Canon/_canonPrim/_primWrapper/_intPrimFQ/PrimTag/_isPrim/_isPrimKeyword
            └── 各自把"int"翻成 i32 / Int32 / Std.Int32 / I32-tag / bool ...

统一后（一元）:
  int  ──ResolveTypeP──▶ Z42ClassType(Std.Int32, IsStruct, Repr=Scalar) ──┐
  Point ───────────────▶ Z42ClassType(Point, IsStruct, Repr=Blob) ────────┤ StructLayout
                                                                            │
  单一入口 PrimModel(keyword) → { fqName:"Std.Int32", canon:"i32", repr:Scalar, irTag:I32 }
            所有旧七表消费点改查它；codegen 产出逐字节不变（Scalar→裸 I64+PrimTag）
```

**触面**：完全封闭在 `z42c.semantics` 单子包（16 src + 4 test）。真逻辑 6 文件（Z42Type / SymbolTable /
EmitContext / TypeChecker / StructLayout / ImportedSymbolLoader），其余机械产出点替换。Rust VM /
syntax / pipeline / ir **零触碰**。

## Decisions

### Decision D1（R1）：类型解析统一 —— 消灭 Z42PrimType

**问题**：基元关键字解析成 `Z42PrimType`（无元数据），与用户 struct 的 `Z42ClassType` 割裂。

**决定**：`ResolveTypeP`（SymbolTable.z42:113）对基元关键字改为**从符号表查已存在的 `Std.*` phantom
struct 的 `Z42ClassType`**（Int32/Int64/Boolean/Char/Double/Single + 各别名 byte/short/... 归到对应 wrapper）。
删除 `Z42PrimType` 类。理由：

- phantom struct **已存在**（z42.core/Primitives/*.z42，`struct Int32 : INumber<int>`，零字段），R1 只是让
  语义类型系统**指向它们**而非造哨兵——符合 philosophy「改产出端根因修复」。
- `Std.*` 类自带 Fields(空)/Methods(CompareTo/op_Add/...)/接口(INumber/IComparable)，方法派发、`is`/`as`、
  泛型约束 `T:INumber` 全部自然可答，不再需要 `_primWrapper` 桥接。

**别名处理**：`byte`→`Std.Byte`(暂无则 canonical `u8` 对应 wrapper) 等，与 canonical 名的映射并入 D3 的
单一 PrimModel 表。**源码写法不变**：`int x` 仍写 `int`，只是内部解析目标从 `Z42PrimType("int")` 变成
`Z42ClassType(Std.Int32)`。

**⚠️ string / object 也是 Z42PrimType（实测 `_isPrim` 含 string/object）**：删 Z42PrimType 必须一并折叠——
`string`→`Std.String`、`object`→`Std.Object`（**引用类型 `Z42ClassType(IsStruct=false)`，非 Scalar 值类型**）。
关键 byte-identical 坑：`string` 现经 `PrimTag("string")→IrType.Str`；若变成普通 Z42ClassType，`ToIrType`
默认返 `IrType.Ref` → **IR 标签从 Str 变 Ref、byte-identical 破**。故 D3 的 PrimModel + D4 的 ToIrType
**必须保留 string→Str、object→Ref 的精确标签**（ToIrType 对内建类型查 PrimModel 拿专属 tag，其余 Z42ClassType
才落 Ref）。string/object 不是 Scalar（无数值算术），但属"有专属 IR 标签/Value 变体的内建类型"，与 bool/char
的不装箱特例同源处理。

### Decision D2（R2）：Repr∈{Scalar, Blob} —— 表示由值类型模型决定

**问题**：统一后不能让每个 `int` 都进字节 arena（性能灾难），必须保住"int 永远是裸 `Value::I64`"。

**决定**：值类型带 `Repr`：

| Repr | 判据 | 运行时载体 | 例 |
|------|------|-----------|-----|
| **Scalar** | 是六个基元 wrapper 之一（零字段、有原生 Value 变体承载） | 裸 `Value::I64/F64/Bool/Char`（**不进 arena**，算术热路径零变） | `int`/`long`/`bool`/`char`/`float`/`double` |
| **Blob** | 多字段 / 含引用叶子 struct（现 `IsBlobStruct`） | 字节 arena（StructLayout 机制） | `Point`/`Line` |

- **Scalar 的确切集合 = 六个基元 wrapper**（Int32/Int64/Boolean/Char/Double/Single）——它们零字段、其"值"就是
  backing 标量。这比 DRAFT 说的"单字段叶子"更精确（基元是**零**字段 phantom struct）。
- 用户零字段 struct（退化，Size==0）与单字段 struct **都算 Blob**——「单标量叶子塌缩」（`struct Id{int v}`→标量）
  留 **Phase 4**，Phase 1 不做。
- codegen 按 Repr 分派：Scalar 走现有基元路径（PrimTag/裸算术/`__box_prim`），Blob 走 StructAlloc/StructCopy。
- **StructLayout 加 `ReprOf(name)`**：wrapper 名→Scalar，`IsBlobStruct`→Blob，其余非值类型→N/A。

### Decision D3：七表收敛成单一 PrimModel 入口

**问题**：`Canon`/`_canonPrim`/`_primWrapper`×2/`_intPrimFQ`/`PrimTag`/`_isPrim`×3/`_isPrimKeyword`/
`_isNumericPrim` 是同一张表的七个投影，散落、语义不一致（`_canonPrim` 保留 int，`Canon` 归 i32）。

**决定**：建单一 **PrimModel 表**（keyword → 记录），字段含：`canonName`（i32/i64/...，重载键用）、
`fqName`（Std.Int32，装箱/身份用）、`irTag`（I32/I64/Bool/...，codegen 用）、`repr`（Scalar）、
`isNumeric`/`isInteger`（拓宽/窄化用）。所有旧投影改成该表的字段读取。

- **保持 canonical 语义不变**（重载键 `Canon` 现产 i32——PrimModel.canonName 沿用，使 `OverloadResolver`
  逐字节不变，这是 codegen 不变式的一部分）。
- 消除 `_canonPrim` 与 `Canon` 的 int/i32 分歧：统一用 `canonName`。**⚠️ 此处若改变任何现有产出会破 byte-identical**
  → 收敛时逐一核对每个消费点的既有行为，收敛是"同义改写"不是"改语义"。

### Decision D4（R4）：算术热路径不变 —— 性能铁律 + codegen 不变式支点

**问题**：性能零回归 + self-host byte-identical。

**决定**：`ExprEmitter._emitBinary`（ExprEmitter.z42:399-429）**一行不动**——它不直接 `is Z42PrimType`，只对
结果寄存器调 `EmitContext.ToIrType`（:508）取 IR 标签。只改 `ToIrType`：`is Z42PrimType` 分支替换为"是 Scalar
值类型 → `PrimTag(其 canonName)`"。这样 `AddInstr`/`SubInstr`/... 全部原样直发裸算术，`Value::I64` 表示不变。

这是**性能零回归**和 **codegen-output-preserving** 的共同结构性支点：Phase 1 全部改动的净效果是"同一个 IR
从不同的内部类型对象产出"，emit 字节不变。

### Decision D5：装箱路由（编译器侧）+ bool/char/double 特例保留

**问题**：int 变成 `Z42ClassType(IsStruct)` 后，`BoxIfNeeded`/`_emitBox` 会不会把它当 struct 走 `__box_struct`？

**决定**：`BoxIfNeeded`/`_emitBox` 的分支从 `is Z42PrimType` 改为**按 Repr**：

- Scalar 整数（int/long/...）擦除到 object/接口 → `__box_prim`（**runtime `Value::Boxed` 不变**）。
- Blob struct → `__box_struct`（**runtime `Value::BoxedStruct` 不变**）。
- **bool/char/double/float/string 不装箱**（`_intPrimFQ` 现返 ""）——**保留特例**：它们有独立 `Value::Bool/
  Char/F64/Str` 变体，自带类型身份，`is`/`as`/`GetType` 无需装箱即可答对。

**Phase 1 绝不碰 runtime 装箱**：`Value::Boxed` vs `Value::BoxedStruct` 的**不对称**（P4b 后 struct 装箱是堆
ScriptObject 引用身份、基元装箱是轻量内联）**保持现状**——DRAFT R3"Scalar 复用 Value::Boxed、Blob 扩展它"的
统一前提已被 P4b 推翻，装箱统一是 **Phase 2** 的独立设计题，不在 Phase 1。

### Decision D6：消除 ImportedSymbolLoader sentinel（根因修）

**问题**：`ImportedSymbolLoader.z42:356` 在导入类型信息不全时降级成 `Z42PrimType` 哨兵（philosophy 点名反模式）。

**决定**：R1 让基元统一走 `Std.*` Z42ClassType 后，导入基元类型直接解析到对应 wrapper 类型（符号表已加载
z42.core 时可查），**删除降级分支**。若确遇"信息尚未加载"的时序问题，走两阶段类型加载 fixup（philosophy 推荐做法），
**不留兼容分支**。⚠️ 若实施中发现根因在 Phase 1 Scope 外（如 TSIG 加载顺序），停下报告 User（越界防护）。

### Decision D7：数值拓宽/窄化迁移

**问题**：`Z42PrimType.IsAssignableTo` 挂着 `_canWiden`（byte/short→int→long→float→double 无收窄）。删了 Z42PrimType 后归谁？

**决定**：拓宽规则迁到**统一值类型的可赋性判断**——`Z42ClassType.IsAssignableTo` 对 Scalar 值类型走数值拓宽表
（PrimModel.isNumeric + 拓宽等级）。窄化放行（`TypeFactsTc.z42:58-60` prim↔prim）同理按 PrimModel 判。**行为
逐字节等价现状**（同一套拓宽/窄化规则，只换承载类型）。

## Implementation Notes

- **实施顺序**（最小化中间态破坏）：① 先建 PrimModel 单一表（新增，不删旧）+ StructLayout.ReprOf；② 把
  `ToIrType`/`BoxIfNeeded`/`ResolveTypeP` 等消费点逐个切到 PrimModel，每切一处 `xtask test compiler` 验
  self-host 不变；③ 全部切完再删 `Z42PrimType` 类 + 旧七表；④ 测试文件机械替换。
- **阶段 3（删 Z42PrimType）关键：`Z42ClassType.Builtin(name)` 轻量合成体**。翻转（2b）只让 `ResolveTypeP`
  产真 phantom（声明类型），字面量/算术/imported 形参/合成 Object 方法等 **30 个 producer 仍产 Z42PrimType**。
  阶段 3 把它们全改产 `Z42ClassType.Builtin(name)`——一个 **Name()=keyword 原样**（"int"/"bool"，**不是**
  wrapper "Int32"）、`IsBuiltin` 恒真、无成员的合成 Z42ClassType，**逐字节复刻 Z42PrimType(name)** 的行为
  （keyword 名 + IsBuiltin 路由，成员访问经 MemberResolver 按名查真 wrapper 类）。**核心教训**：合成体的
  Name() 必须是 **keyword**，因编译器内部大量逻辑按 `Name()` 比较/Dump/收集（`f.FieldType.Name()=="int"`、
  `sig.Dump()=="(int,int)->int"`）——用 wrapper 名会全漂。
- **删 Z42PrimType 的连带工作 = 让每个「判 primness」的消费者同时认 keyword + wrapper 名**（经 PrimModel.
  Canon/Keyword/IsScalarValue 归一），因翻转前它们只见 Z42PrimType（keyword 名）、翻转后见合成 ClassType
  （keyword）+ 真 phantom（wrapper）。逐个揪出的点：`OverloadResolver._assignable`（object 装箱按 Canon）、
  `MemberResolver` stub 受者（imported 兜底对未加载真类名产空方法 stub → 重解析/loose 绑定复刻 prim 路径）、
  `BinaryTypeTable._structPrimName`（IsBool/IsNumeric/IsOrderable 认 keyword）、`ConstraintChecker._isClassArg`
  （scalar 合成体不满足 `where T:class`）、`Conversion` 拆箱须在通用 class→class 分支**之前**（object/int 均
  ClassType，否则 class→class IsSubclassOf 皆 false 误返 None）。**gen1 能否自编译 z42.core/z42.ir 是最强早期
  信号**（比全量 test 快数量级；每改完先 warm gen1→gen2 看能否自编译）。
- **⚠️ 翻转后核心不变式：`Z42Type.Name()` 写入任何持久元数据的边界必须过 `PrimModel.SurfaceName`**（翻转
  前内建 leaf 是 `Z42PrimType("int")`、`Name()`="int"；翻转后是 `Z42ClassType("Int32")`、`Name()`="Int32"）。
  内建 wrapper 名（Int32/String/Object/Boolean…）**绝不可泄漏进 zpkg 元数据**，否则破坏 byte-identical +
  下游误解析。共 **3 个泄漏边界**（阶段 2b GREEN 时逐个揪出）：
  1. **TSIG 导出**（`ExportedTypeExtractor._resolvedTypeName`）：递归 array/instantiated/func + leaf 过
     SurfaceName。泄漏后果：上一 nightly baseline seed 冷启动消费本包 TSIG 时不认 wrapper 名 → E0402。
  2. **struct 布局字段拼写**（`SymbolCollector` OwnFieldSpelling，:699/709）：喂 `StructLayout.FieldTypeName`
     → codegen 每次字段访问 `Tag.FromName(FieldTypeName)`；`Tag.FromName` 只认 keyword，wrapper "Int32"
     回落 `Tag.Object`(ref) → 给 scalar 字段 baked ref-kind → 运行期 `get_ref` 越界（ref_offsets 由
     Canon-aware 的 `_kindOf` 算对、baked kind 却错，二者不一致）。
  3. **SIGS 段**（`FunctionEmitter._sigTypeName` :211 + lambda retName :308）：内建 leaf 走
     `ResolveTypeP().Name()` → SIGS 泄漏 `Int32[]`/`Object[]`。**最隐蔽**——不直接报错，而是让「用新 z42c
     重建的 stdlib」带残差类型名，导致 z42c **对该 stdlib 重新自编译**时产出行为错误的 z42c（`params
     object[]` 的 `int→object` 装箱判定失活 → varargs 不打包）。制造「warm build（对 baseline stdlib）过 /
     cold full-test（对重建 stdlib）不过」的假象。定位法：**收敛链最小验证**（build→rebuild stdlib→build
     against it→跑最小用例），比盲跑全量 `xtask test` 快数量级。
  > 结论：翻转是「窄口收敛」——`ResolveTypeP` 一处改产 phantom class，但**表面名投影散落在 N 个 emit 边界**，
  > 每个都要 SurfaceName 兜回 keyword。self-host 不动点（gen1==gen2）**测不出 SIGS 泄漏**（gen 间都写
  > wrapper、彼此相同），唯有「对 baseline seed 冷启动」或「对 baseline stdlib 对账」才暴露。
- **byte-identical 验证是主门禁**：每个中间步骤都跑 `xtask test compiler`（self-host 5/5 gen1==gen2）。**不传
  `Z42_HOME`**（血泪教训 [[struct-value-semantics-program]]）。
- **z42c 局部变量非块作用域 shadow**（compiler-z42c.md 踩坑）——收敛引入的新局部变量名勿与外层同名。
- **无格式 bump**：warm 本地全程可验（种子 0.37==源，无两代自举墙）。worktree z42-uvt 需先播种 `.z42`
  （见 tasks 环境准备）。

## Testing Strategy

- **单元测试**：`z42c.semantics/tests/types/type_tests.z42` 断言 `ResolveType("int")` 返回 `Z42ClassType`
  且 `IsStruct && Repr==Scalar && Name()=="Std.Int32"（或 canonical）`；重载/可赋性/拓宽用例保持通过。
- **codegen 不变式（核心）**：`xtask test compiler` self-host **gen1==gen2 byte-identical** + 全量 golden
  （`xtask test e2e` + cross-zpkg + stdlib）逐字节/逐输出不变——这是"纯重构"的证明。
- **VM 验证**：`xtask test`（不传 Z42_HOME）完整 GREEN gate。
- **bootstrap 边界**：无新语法/格式 → `xtask test bootstrap` 应无越界（上一 nightly 能编当前源）。
