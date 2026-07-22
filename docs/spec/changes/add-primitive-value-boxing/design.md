# Design: 基元值类型装箱（方案 A，一步全装箱）

> 配套 [proposal.md](proposal.md)。User 定：方案 A + 一步全装箱。

## 表示：新增 `Value::Boxed`

`Value` payload 上限 8 字节放不下「宽度 tag + i64」内联 → 走堆。新增变体：

```rust
Boxed(Box<BoxedPrim>) = 11,
struct BoxedPrim { class: Arc<str>, inner: Value }   // class = FQ 基元 struct（Std.Int64/…），inner = 裸基元值
```

- 只在**装箱点**创建；算术/方法体永远拿 `inner`（拆箱后），故热路径零影响。
- `Box<…>` 是 8 字节指针 → 不撑大 Value。inner 为基元（I64/F64/Bool/Char/Str）。

## IR：新增 `Box` / `Unbox` 指令

- `Box <classFQ> %src → %dst`：`%dst = Boxed{class, %src}`。编译器在 prim→object/接口 转换点发。
- `Unbox <classFQ> %src → %dst`：`%src` 为 Boxed 且 `is_subclass_or_eq_td(boxed.class, classFQ)` → `%dst = inner`；否则异常（硬转）/ Null（`as`）。

## 编译器：装箱/拆箱插入点

**装箱**（prim 静态类型 → object/接口 静态类型 的隐式转换）——当前**无转换节点**（裸 copy），须在
TypeChecker 的赋值兼容处，对「源 prim + 目标 object/接口」插入 `BoundBox`：
- 赋值 `object o = 5L` · 实参传递（形参 object/接口）· return（返回类型 object/接口）· 数组/集合元素存
  （`object[]`）· 插值/字符串拼接的 object 参 · 三元/switch 结果类型统一到 object。
- 判据：`src.Type() 是基元` 且 `target 是 object 或接口` → 包 `BoundBox(src, primFQ)`。
- `BoundBox.Type()` = 目标（object/接口）；emit `Box <primFQ>`。

**拆箱**（object/接口 → prim 的强转 `(long)o` / `o as long`）：
- `_emitConvert`（`(T)o`）：源为 object/接口、目标 prim → emit `Unbox`（而非 numeric ConvertInstr）。
  源为数值、目标数值 → 仍 ConvertInstr（不变）。
- `_emitCast`（`o as long`）：目标 prim、源 object → `Unbox`（不匹配 → Null）。
- `is`：`o is long` 已 emit IsInstance；运行时对 Boxed 走 `is_subclass_or_eq_td(class, target)`（见下）。

**关键消歧**：`(int)x` 现有两义——① x 是 object → 拆箱；② x 是 long → 数值窄化。按 `x.Type()` 分派：
object/接口→Unbox，数值→Convert。618 处现有 cast 多为②数值，不受影响；仅①object 源改走 Unbox。

## 运行时：interp + jit 全路径

| 操作 | Boxed 行为 |
|------|-----------|
| `Box` exec | 分配 `Boxed{class, inner=src}` |
| `Unbox` exec | is-a 校验 → 返 inner；否则异常/Null |
| `is_instance` | `Value::Boxed(b) => is_subclass_or_eq_td(registry, &b.class, target)`（走真 type_desc：base+接口链）|
| `as_cast` | Boxed 且 target 是其类/基/接口 → 若 target 是 object/接口**保持 Boxed**，若 target 是 prim**拆箱返 inner**；否则 Null |
| `GetType` | 从 `b.class` 造 Std.Type |
| `vcall` | 解析 `b.class` 的方法，`this = b.inner`（拆箱后交基元 struct 方法体）|
| `Equals`/`ToString`/`GetHashCode` | 经 vcall → 基元 struct 方法，`this=inner` |
| `field_get`（如 boxed 无字段）| N/A（基元无实例字段）|
| GC trace | inner 为基元/Str(Arc) → 无 GC ref，免 trace（或 trace inner 保守）|

`prim_isa`（fix-boxed-primitive-is-as 的裸基元松匹配）**保留**给**未装箱**裸 `Value::I64`（如未经 object
边界的场景）；装箱后精确匹配走 Boxed 臂。→ 强类型：`object l=9L; l is long` 装箱为 `Std.Int64` → true；
`object x=5; x is long` 装箱为 `Std.Int32` → **false**。

## 自举/迁移风险与缓解

- **z42c 已手工 `IntBoxZ` 装箱**（不靠裸-prim-in-object）→ 主要热点不破。
- 剩余风险：某处「prim 隐式流进 object 再非 cast 读回」——强类型本应 forbid；审计 grep：object 形参/字段
  被当 prim 直接用（无 `(T)`/`as T`）。self-host 5/5 byte-identical 是硬 gate：z42c 自编若因装箱语义
  变化产出不同 → 立即暴露。
- **分步验证**：先落 runtime（Value::Boxed + 5 处 handling）编过 + cargo test → 再落 compiler emit →
  每步 self-host 5/5。红则定位单点。

## 格式 bump：**已规避**（改用现有 opcode，2026-07-22 更正）

最初担心 Box/Unbox 是新 IR opcode → zbc 格式 bump。**实测可规避**——复用现有稳定 opcode，**无格式变更**：

- **Box** → `BuiltinInstr`（opcode 0x51，现有）：编译器在 prim→object/接口 点发
  `const.str %c "Std.Int64"; builtin __box_prim %dst, %value, %c`。运行时新增 `__box_prim`
  builtin（value + class 串 → `Value::Boxed`）。builtin 是命名 Call，**不改格式**。
- **Unbox** → 复用现有 `AsCast` opcode：`o as long`（AsCast Std.Int64）对 Boxed 值 → 匹配则返
  `inner`（拆箱），否则 Null。`(long)o` 硬转同理经 as/convert 消歧。**零新 opcode**。
- **is / GetType / vcall** → 现有 opcode，只在运行时加 Boxed 臂。

→ **纯运行时 + 编译器逻辑改动，无格式 bump、无两代自举**。复杂度回落到与 fix-boxed-primitive-is-as
同量级（+ 装箱插入点的类型检查改动）。

## 实施进度（2026-07-22）
- [x] runtime：`Value::Boxed(Box<BoxedPrim>)` 变体 + trace_children / value_to_str / object_size_bytes
      三处 exhaustive-match 处理（编过，惰性——无 Box 指令发射前零行为变化）。
- [ ] Box/Unbox IR opcode（Rust `Instruction` + zbc 编解码 + z42.ir ZbcWriter/Reader；**格式 bump**）
- [ ] interp/jit：Box/Unbox exec + is/as/GetType/vcall/Equals 的 Boxed 臂
- [ ] compiler：BoundBox + 装箱插入（TypeChecker 强转兼容处）+ Unbox 消歧 emit
- [ ] gate：self-host 5/5 + cargo + test compiler + 强类型 golden

## 分阶段实施（虽「一步」范围，实施仍按依赖序）
1. runtime：Value::Boxed + Box/Unbox exec + is/as/GetType/vcall/Equals（interp）。
2. runtime jit：同上 helper。
3. compiler：Box/Unbox IR + BoundBox/装箱插入 + Unbox 消歧 emit。
4. gate：self-host 5/5 + cargo + test compiler + is/as 强类型 golden（扩 boxed_primitive_is_as：
   `5 is long`=false / `9L is long`=true / GetType / boxed.ToString / boxed Equals）。

## 实施进度快照（2026-07-22，claude/z42c-ctor-arity）

用 `__box_prim` builtin（非新 opcode，规避格式 bump）+ var-decl 装箱插入。**整数强类型
is/as/GetType 在 interp + jit 两模式实测通过、self-host 收敛、runtime 782 单测绿、golden 更新绿。**

**canonical 例（interp==jit 一致）**：
```
object l = 9L;   l is long=true   l is int=false   l.GetType().Name=Int64   l as long=9
object by=(byte)7; by is byte=true  by is int=false  by.GetType().Name=Byte
object i = 42;   i is int=true    i is long=false  i.GetType().Name=Int32
```

**已完成**：
- ✅ 缺口①：字面量后缀定宽（`ExprTyper._bindLiteral`：`L/l`→long、`U/u`→unsigned、`UL`→ulong；
  无后缀按 int 界）。`9L`→Std.Int64（曾误判 int→Std.Int32）。**self-host 验证 gen2==gen3
  逐字节收敛**（z42c 源自带 `0L/1L` 枚举字面量 → gen1 一代过渡差异，gen2 起稳定；旧 nightly
  仍能编当前源，无越界）。
- ✅ 缺口④：GetType 保留装箱类——vcall Boxed 臂特判 `GetType`，直接交 `builtin_obj_get_type`
  （用 `b.class`），不拆箱 `this`（否则按 inner 默认类报告丢宽度 → 曾 Int64 误报 Int32）。
- ✅ 缺口②：JIT Boxed 臂——`jit/helpers/object.rs`（is_instance/as_cast）+ `jit/helpers/vcall.rs`
  （vcall + GetType 特判），镜像 interp。interp==jit 一致。
- ✅ object 短路：装箱基元 `is object` / `as object`——基元类名（`Std.Int32`）在 type_registry
  未必可解析到 `Std.Object` 基链（真名 `Std.Primitives.Int32`），故 is_instance/as_cast 的 Boxed
  臂对 object 目标显式判真（interp+jit，镜像 prim_isa）。修 var-decl 装箱引入的 `is object` 回归。
- ✅ golden：`src/tests/types/boxed_primitive_is_as.z42` 松匹配→强匹配（`l is int` true→**false**
  + GetType 断言），interp+jit 均绿。
- ✅ `corelib::tests` 更新：`__obj_get_type` 对裸基元返 Type（不再 error）。

**剩余（未达完整 GREEN 前）**：
1. **缺口③（待 User 裁决）**：非整数基元（string/bool/char/double）**按设计不装箱**，其
   object-typed `.GetType()` 走 primitive vcall 路径找不到 `Std.Object.GetType` → bail
   `expected object, got Str`。疑似 **pre-existing**（与整数装箱正交）。选项：① 给 primitive vcall
   路径补 GetType 兜底；② 确认 pre-existing 另开 change。
2. 其余 coercion 插入点（call-arg / return / array-store / 三元）——目前仅 var-decl 一处；
   未装箱处走裸基元 prim_isa 松匹配（不破但不一致）。
3. gate 补全：`xtask test compiler` [Test] 单元 + e2e 全量 golden（本地已验 runtime 单测 + 目标
   golden + self-host；全量 e2e/test-compiler 待跑）。

> **状态**：**整数装箱强类型端到端可用（interp+jit）**，self-host 收敛，目标 golden + runtime 单测绿。
> 缺口③ 待裁决、其余 coercion 点 + 全量 gate 待补 = 完整 GREEN 的剩余项。
