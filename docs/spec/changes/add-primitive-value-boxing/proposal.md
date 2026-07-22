# add-primitive-value-boxing — 基元装箱：`object` 保留基元的精确类型（强类型 is/as/GetType）

> 状态：**DRAFT（设计待 User 裁决，未实施）** | 创建：2026-07-22
> 触发：`fix-boxed-primitive-is-as` 的整数宽度松匹配（`5 is long` 也返 true）不符「简单强类型」诉求。
> 类型：lang / vm 设计——**规范先行**。占用 `compiler` + `runtime`。

## 问题

z42 运行时**不装箱基元**：`object o = 5L` 编译成一条裸 `copy`，`5L` 以裸 `Value::I64` 流进
object 槽——**丢失静态类型**（int/long/short/byte 运行时同为 `Value::I64`）。于是 boxed 基元的
`is` / `as` / `GetType` 无法辨别宽度：

```z42
object l = 9L;
l is long;   // 想要 true
object x = 5;
x is long;   // 想要 false（当前 fix-boxed-primitive-is-as 松匹配返 true——不够强类型）
```

`fix-boxed-primitive-is-as` 已让「boxed 整数 is-a 任意整数类型」止血（永不假阴），但那是**松匹配**，
与 User「简单的强类型」诉求相悖：`5 is long` 不该为 true。要真正强类型，必须让 `object` **保留**
被装入值的精确基元类型。

## 根因

1. **无装箱点**：prim→`object`/接口 的隐式转换编译成裸 `copy`（IR 实证：`object o = 5L` →
   `%1 = copy %0`，无 box/convert）。裸 `Value::I64` 无处承载「这是 long 不是 int」。
2. **表示无处放宽度**：`Value` 是 tagged union，payload 上限 8 字节（`I64(i64)` 已占满）。
   给 `Value::I64` 加内联宽度 tag 会撑大所有 Value（伤全局），或需 8 个窄整数变体（丑）。
   → 承载精确类型只能走**堆装箱**（把基元装进带类型的堆对象）。

## 关键事实（利好）

z42 的基元**已是真 struct**（`src/libraries/z42.core/src/Primitives/Int32.z42`：
`public struct Int32 : IComparable<int>, IEquatable<int>, INumber<int>`，Int64/Byte/… 同）。
它们有真 type_desc、接口表、vtable。**装箱成「带 Int64 type_desc 的堆对象」→ is-a / GetType /
接口判定 / vcall / Equals 全走现成 Object 机制，零新机制**——这正是 .NET/JVM 的值类型装箱模型，
且与 z42 用户 struct（本就是堆 `Value::Object`）行为**统一**。

## 方案（推荐：值类型装箱，复用 Object 机制）

**在 prim→`object`/接口 转换处装箱，在 `object`→prim 强转处拆箱。**

- **装箱**：编译器在「基元静态类型 → object/接口」转换点 emit `Box <FQ基元struct>`（如
  `Std.Int64`）；运行时分配一个 `ScriptObject`，type_desc = 该基元 struct，裸值存其 native data
  （或单字段）。`9L` → 带 `Std.Int64` type_desc 的堆对象；`5` → 带 `Std.Int32` 的。
- **拆箱**：`(long)o` / `o as long` 在「object→prim」处 emit `Unbox`；运行时读回裸 `Value::I64`。
- **is / as / GetType / 接口 / vcall / Equals / ToString**：boxed 基元是**真 Object**，全走现成
  Object 路径——`l is long` 查 type_desc.name==`Std.Int64` → true；`x is long` type_desc==
  `Std.Int32` → **false**（强类型达成）。
- **未装箱基元照旧**：裸 `Value::I64` 仍走 `primitive_class_name` 分发（算术、方法调用零改、零装箱开销）。
  装箱**只**发生在显式/隐式转 object/接口的边界（罕见），热路径不受影响。

## 备选（供裁决）

| 方案 | 强类型 | 简单度 | 代价 |
|------|--------|--------|------|
| **A. 值类型装箱（推荐）** | ✅ 精确 | 中（复用 Object 机制，仅加 Box/Unbox 转换点 + 迁移） | 装箱堆分配（罕见）；**语义变更风险**：现有「靠裸 I64 流过 object 再直接读」的代码须改显式拆箱——强类型本应强制，但需审计+测 |
| **B. 内联窄整数变体** | ✅ 精确 | 低（撑大 Value 或加 8 变体 + 全 match 处理） | Value 变大伤全局 / 变体爆炸 |
| **C. 松匹配（现状 fix-boxed-primitive-is-as）** | ❌ `5 is long`=true | 高 | 不满足强类型诉求 |

## 建议

**采 A**：它是「简单的强类型」的最佳点——**唯一新概念是转换边界的 Box/Unbox**，其余（is-a/
GetType/vcall/GC）全复用现成机制；与用户 struct 装箱统一；符合 .NET/JVM 心智。分阶段落地 +
充分迁移测试控风险。C（现状松匹配）作为 A 落地前的过渡保留。

## 待裁决点（design.md 展开）

1. 装箱值存 ScriptObject 的 **native data** 还是**单 value 字段**？
2. **接口装箱**是否本轮纳入（`object` 与 `IComparable` 同为装箱触发）？
3. **迁移策略**：如何审计现有「裸基元流经 object」的点（grep `object` 参数 + 无显式 cast 读回）？
4. 分阶段：先 is/as/GetType（观测缺口）→ 再 vcall/Equals/ToString 全装箱语义？
