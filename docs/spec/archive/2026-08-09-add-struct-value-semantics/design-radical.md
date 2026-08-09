# Design: 统一值类型模型（B-radical）—— `int` ≡ `Std.Int32`

> 状态：🟡 架构 DRAFT — User 裁决 B-radical（基元也是值类型，CLR 模型）。本文把 struct 值语义、
> packed 数组、基元统一收敛成**一个值类型模型**。基于内脏调查（file:line 见下）。**需 User 审这份
> 架构 → 确认 → 再拆实施阶段 → 才写码**（lang/vm 重架构，需规范先行）。

## 关键事实（调查结论）

现状 `int` 与 `Std.Int64` **已半统一**，非两个从零的世界：

| | `int`（基元世界） | `Std.Int64`（struct 世界） |
|---|---|---|
| 编译期 | `Z42PrimType{"int"}`（纯名字，[Z42Type.z42:32]） | `Z42ClassType{IsStruct=true}`（完整元数据+方法，[Z42Type.z42:51]） |
| 运行时 | 裸 `Value::I64`（无宽度、无 TypeDesc，[types.rs:659]） | 真 TypeDesc + `CLASS_FLAG_STRUCT`；**零字段 phantom struct**，`this`=裸标量 |
| 算术 | 直发 `AddInstr` 直加 I64（[ExprEmitter.z42:314]/[exec_value.rs:57]） | 不参与（`op_Add` 仅为泛型 `T:INumber` 存在） |
| 接触点 | 名字表 `Canon`/`_primWrapper`/`_intPrimFQ` · 方法派发 · boxing（仅整数） | 同左 |

已有可复用基础设施：**StructLayout**（字节精确布局+种类引用位图，阶段1 已建）、**ArrayBacking**
（packed 数组字节 typed backing：Bool=1/Bytes=1/I32=4/I64=8/Chars=4/F64=8，[types.rs:438]）、
phantom struct + `Value::Boxed{class,inner}` 装箱（[types.rs:733]）+ `INumber` 接口。

> **B-radical = 补完统一，复用这些，不重造。** 三件事（struct 值语义 / packed 数组 / 基元统一）
> 归一为**一个字节精确值类型模型**。

## 统一模型（一句话）

**每个值类型都有一份 StructLayout（字节精确）。表示由布局决定：单基元叶子 → 标量表示（复用裸
`Value::I64/F64/Bool/Char`，算术热路径不变）；多字段 → blob 表示（字节 arena）。复制/装箱/FFI/GC 全
由「布局 + 种类引用位图」统一驱动。`int`=单叶子值类型 `Std.Int32`（标量表示）；`Point`=多叶子值类型
（blob 表示）。**

## Decisions（架构，⚠️=最需 User 拍板）

### ⚠️ R1：类型系统统一 —— 消灭 `Z42PrimType`/`Z42ClassType` 二元

- `int`/`long`/`bool`/`char`/`float`/… 解析到其 `Std.*` 的 **`Z42ClassType`（IsStruct=true）**，
  不再是 `Z42PrimType`。`Canon` 从"别名归一"升级为"关键字 → `Std.Int32` FQ 类型"。
- 所有 `x is Z42PrimType` 分支（[ExprEmitter.z42:77,109,890]、[ExprTyper.z42:455] 等，见调查 G1）
  改为"值类型 Z42ClassType + 表示查询"。`_primWrapper`/`_intPrimFQ` 三表收敛成一个"关键字→值类型"入口。
- 代价：编译器较大重构（类型解析/可赋性/重载/反射），但概念干净、消除三张桥接表。

### ⚠️ R2：表示策略（标量 vs blob）—— 核心

值类型的**表示**由 StructLayout 派生：

| 布局 | 表示 | 运行时载体 | 例 |
|------|------|-----------|-----|
| 单基元叶子（1 字段、基元、无引用叶子） | **标量** | 复用裸 `Value::I64/F64/Bool/Char`（**不进 arena**，算术热路径零变） | `int`/`long`/`bool`/`char`/`float` |
| 多字段 / 含引用叶子 | **blob** | 字节 arena（StructLayout 机制，阶段2） | `Point`/`Line`/`Box` |

- 编译器给每个值类型标 `Repr∈{Scalar,Blob}`（由 layout 判：`FieldCount==1 && 唯一叶子是 Prim && size≤8`
  → Scalar）。codegen 按 Repr 分派：Scalar 走现有基元路径，Blob 走 StructAlloc/StructCopy。
- **这是"单字段值类型标量替换"**——保证 `int` 永远是 `Value::I64`，不会每个 int 进字节 arena（否则性能灾难）。

### ⚠️ R3：运行时类型/宽度 + 装箱统一

- 标量表示的值仍不在 `Value` 里带宽度（`Value::I64` 辨不出 int/long）。但统一后**静态类型恒已知**
  （int 就是 Std.Int32），所有静态调用点 emit 精确类型 → [exec_vcall.rs:80] "裸 I64 默认 Int32 丢宽度"
  的回落点在静态路径不再触发。
- **动态类型身份**（值进 `object`/接口 槽）仍需精确类型 → **boxing 保留，但统一**：装箱任何值类型 →
  堆 `ScriptObject`（Scalar：包 scalar+type；Blob：blob→堆）。现有 `Value::Boxed{class,inner}`
  **本就是"标量值类型装箱+精确类型"**（[types.rs:733]）→ 可作为 Scalar 装箱的现成载体，Blob 装箱扩展它。
- 现有"bool/char/double 不装箱"特例（[TypeChecker.z42:38]，因它们是独立 Value 变体、运行时可辨）
  在统一模型下重新评估：可保留（独立变体天然带类型）或并入统一装箱。**倾向保留**（零成本、不回归）。

### R4：算术 lowering 不变（性能铁律）

- `int+int` **仍直发 `AddInstr`** 直加 `Value::I64`（[ExprEmitter.z42:314]/[exec_value.rs:57] 不动）。
  类型系统说 int 是 Std.Int32，但 codegen 对 Scalar 值类型**快路径直发原生算术**。
- `op_Add` 方法继续为泛型 `T:INumber` 存在（[INumber.z42]）。裸算术与泛型 op_Add **现状已并存**，统一后
  维持：Scalar 直发、泛型 VCall op_Add。**无热路径回归**。

### R5：FFI / C interop（统一的大 payoff）

- 值类型有字节精确布局 → **直接 marshal 到 native、零装箱零拷贝**。Scalar 复用现有
  `value_to_z42`（[marshal.rs:83]）；Blob struct 按 layout 字节直传（现状 struct 不能跨 FFI，
  [marshal.rs:1]）。与 packed-array 的 `ArrayBacking`（[types.rs:438]，byte[] 已 `&[u8]` 直传，
  [exec_native.rs:136]）**共用同一字节模型**——这正是 [[packed-primitive-arrays]] 的动机合流。

### R6：GC / 引用位图统一

- Scalar 值类型无引用叶子（基元）→ GC 无关。Blob 值类型按 StructLayout 种类引用位图扫描
  （阶段2 Decision ζ）。`ArrayBacking::Boxed` vs typed backing 的 GC 区分与此同源。

### R7：判定点收敛

调查 G6-G10 列的所有"值/引用/基元"判定点（`CLASS_FLAG_STRUCT`/`is_value_type_primitive`/
`primitive_class_name`/`PRIM_TYPE_*`/`prim_isa`/`is_integer_class`）**收敛到单一来源**：
"该类型的 StructLayout（是不是值类型 + 表示 + 字节布局 + 引用位图)"。运行时 TypeDesc 携带之。

## 重塑的程序（把三件事并成一个）

| 阶段 | 内容 | 说明 |
|------|------|------|
| **✅ P1-已做** | StructLayout 字节精确布局（阶段1） | 值类型模型的共同地基，已落地 |
| **A. Blob 值类型**（=原 阶段2/3） | 多字段 struct：字节 arena + StructCopy/字段访问 + GC + zbc 格式 bump | **不碰基元**（基元维持现有 phantom-struct 模型不动）；先落地用户 struct 值语义 |
| **B. 基元类型统一**（radical 核心） | R1 类型统一 + R2 标量表示形式化 + R3 装箱统一 + R5 FFI；替换现有 `Value::Boxed`/名字桥接表 | 风险最高、触面最广；**替换而非桥接**现有基元模型（无 throwaway） |
| **C. packed 数组收敛** | `struct[]` 字节 backing 与 ArrayBacking 统一（原 P3 + packed-array） | 值类型 + 数组字节模型合流 |
| **D. JIT + 反射 + 收尾** | JIT 值路径、反射一致、bench | interp 全绿后 |

> **为什么 A 先于 B（关键）**：A（blob struct）**不需要碰基元**——基元保持现有已工作的 phantom-struct
> 模型，直到 B 才**整体替换**它。这**不是**建"会被推翻的桥"（User 拒绝的那种）——现有基元模型是既存的、
> 一直工作着的，A 阶段不给它加任何新桥接；B 阶段直接用统一模型替换。三件事共用 StructLayout，无返工。

## 复用清单（不另起）

- `StructLayout`（[StructLayout.z42]，阶段1）：字节布局 + 种类引用位图 —— 值类型模型地基。
- `ArrayBacking`（[types.rs:438]，packed-array）：字节 typed backing —— 数组/FFI 字节模型。
- phantom struct + `Value::Boxed`（[types.rs:733]）+ `INumber`（[INumber.z42]）：基元 struct 现有模型。
- 归档参考：`add-primitive-as-struct`（2026-04-23）、`add-primitive-value-boxing`、
  `fix-boxed-primitive-is-as`、`add-generics-g4b-primitive-interface`。

## 风险 / Open Questions（本架构 gate 待 User 裁决）

- [ ] **R1**：消灭 Z42PrimType（int→Std.Int32 Z42ClassType）——接受这个编译器重构面吗？
- [ ] **R2**：表示策略"单基元叶子→标量、其余→blob"（保证 int 仍 Value::I64、算术零回归）——认可吗？
- [ ] **R3**：装箱保留并统一（Scalar 用现有 Value::Boxed 载体、Blob 扩展）；bool/char/double 不装箱特例保留——认可吗？
- [ ] **程序重塑 A→B→C→D**：先 blob 值类型（不碰基元）、再基元统一（替换现有模型）、再 packed 收敛——接受这个排序吗？
- [ ] **是否重命名 change**：`add-struct-value-semantics` → 更宽的 `unify-value-types`（含基元统一）？还是保持并扩展现容器？
