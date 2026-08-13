# Design: R3 装箱统一（基元 → 堆 ScriptObject + 引用身份）

## Architecture

统一后**唯一装箱模型** = `Value::BoxedStruct(GcRef<ScriptObject>)`：
- struct 装箱：`ScriptObject{ type_desc=struct 类型, struct_bytes=blob 基元叶子, struct_refs=引用叶子 }`（P4b 现状，不动）。
- **基元装箱（新）**：`ScriptObject{ type_desc=基元 wrapper 类型(Std.Int32/Std.Boolean/…), <裸标量存储> }`。

`__box_prim` 现状（`corelib/convert.rs:9`）产 `Value::Boxed(Box<BoxedPrim{class, inner}>)`；改成走
`box_struct_blob` 同款路径 alloc `ScriptObject` → 返 `BoxedStruct`。删 `Value::Boxed`/`BoxedPrim` 后，
全部 `match { Value::Boxed(b)=>…, Value::BoxedStruct(gc)=>… }` 双写收敛成单臂。

## Decisions（⏳ = 待 User 确认）

### ✅ D1（User 裁决 2026-08-13）：**D1-B —— struct_bytes 存裸字节（完全同构）**
基元装箱的 ScriptObject **裸标量存 `struct_bytes`**（与 struct 装箱完全同构）。**实现取零格式 bump 路径**：
boxed-prim 的 `struct_bytes` 尺寸**运行期按 wrapper 名推标量宽度**（Int32→4 / Int64→8 / Boolean→1 /
Char→4 / Double→8…，一张 `wrapper→scalar_width` 表），**不改 wrapper 类型的 emitted struct_layout**（Int32
phantom 仍零字段/size 0，不动 zbc TYPE section）→ **无格式 bump、不碰 [[rebuild-class-access-on-unify]]**。
拆箱按同款宽度从 `struct_bytes` 解码回裸 Value。反射（GetFields 空——Int32 无字段，标量即 this）不迭代字段，
把整个 boxed-prim 当标量读。⚠️ 与 Phase 4「单标量塌缩」是同一机制的两半（本 Phase 只做装箱侧 struct_bytes 承载，
塌缩=优化留 Phase 4），可复用本 change 的 `wrapper→scalar_width` 表。**若实现中发现必须动 wrapper struct_layout
（格式 bump）→ 停下报 User**（因会与「重建类访问」的 0.38 bump 撞车、需协调）。

<details><summary>（存档）D1-A 备选：slots[0] 存 Value（未选）</summary>

### ~~D1-A：裸标量在 boxed-prim ScriptObject 里存哪~~
基元 wrapper（`Std.Int32`）是 **phantom struct，零字段，this=裸标量**——无字段槽可放标量。两个选项：

- **D1-A（推荐）：存 `slots[0]` 作 `Value`**。boxed-prim = `ScriptObject{ type_desc=Int32, slots=[Value::I64(v)] }`。
  拆箱 = 读 `slots[0]`。**最简**（无字节编解码、保留标量 Value 类型身份、GC 天然 visit slots）；代价 = 与
  struct 装箱的 `struct_bytes` 存储形态**不完全同构**（但都是 BoxedStruct、引用身份/反射/GC 统一，仅 payload 位置不同）。
- **D1-B：存 `struct_bytes`（裸字节）**。给 wrapper 类型一个「单标量叶子」runtime StructLayout（size=标量宽度），
  标量编码进 `struct_bytes`。**完全同构** struct 装箱；代价 = 需给 wrapper 造 scalar layout + 字节编解码 +
  可能触达格式（wrapper 的 struct_layout 元数据）→ **有格式 bump 风险**。
- **权衡**：D1-A 零格式 bump、改动小、语义等价（引用身份靠 BoxedStruct 的 GcRef，与标量存哪无关）；D1-B 更「纯粹
  统一」但引入格式风险 + 与 Phase 4「单标量塌缩」重叠。**倾向 D1-A**，把「标量↔blob 完全同构」留给 Phase 4。

</details>

### D2：boxed-prim 的 `type_desc` 来源
`__box_prim(裸值, 类名Str)` 的 arg1 已是 FQ wrapper 名（`Std.Int32`…）→ `ctx.try_lookup_type(类名)` 取
`TypeDesc`。该 wrapper 类型**恒已加载**（z42.core 恒在依赖闭包）——但需验：boxed-prim 反射 `GetType()` 应返
`Std.Int32` 的 Type（现 `Value::Boxed` 走 `make_type_from_name(b.class)`，统一后走 `gc.type_desc().name` 同结果）。

### D3：引用身份 = C# 语义
每次 `__box_prim` alloc **新** ScriptObject（GcRef）→ `object o=5; object p=5; ReferenceEquals(o,p)==false`
（两个不同盒，C# 对齐）。别名（`object q=o`）clone GcRef → 同盒。**与 struct 装箱 P4b 引用身份逐条同款**。

### D4：拆箱 / 相等 / ToString / GC / 反射 收敛
- **拆箱**（AsCast object→int / `__unbox`）：从 boxed ScriptObject 读回裸标量（D1-A：`slots[0]`）。现 `Value::Boxed(b)=>b.inner`
  改 `Value::BoxedStruct(gc)=>gc.borrow().slots[0]`（基元盒）——需与 struct 拆箱（拷 blob 回 arena）**按 type_desc 分流**
  （基元 wrapper→读标量；多字段 struct→拷 blob）。
- **equality**：boxed-prim 值相等（现 `Value::Boxed` 按 inner 比）；引用相等（`ReferenceEquals`）按 GcRef 指针。
- **ToString / GetHashCode / GetType / 反射 GetValue/SetValue**：所有现 `Value::Boxed(b)` 臂改读 boxed ScriptObject。
- **GC visit**：`Value::BoxedStruct(gc)=>visit slots+struct_refs`（现有）自动覆盖基元盒（slots[0] 被 visit）。

### D5：删 `Value::Boxed` + `BoxedPrim`
全部收敛完、cargo 单测绿后，删 `Value::Boxed(Box<BoxedPrim>) = 13` 变体 + `BoxedPrim` struct。⚠️ Value 变体
**判别值**（`= 13`）若被 JIT/序列化依赖需查（大概率纯内存态、不序列化）。删后 Value 变体 -1。

## Implementation Notes

- **实施顺序**：① `__box_prim` 改产 BoxedStruct（保留 `Value::Boxed` 变体，先让新盒跑通）→ cargo test；
  ② 逐个把 `Value::Boxed(b)` helper 臂改成 BoxedStruct-基元臂（拆箱/相等/ToString/反射/GC/convert/repl）→
  每改 `cargo test --lib`；③ 全绿后删 `Value::Boxed`/`BoxedPrim` 变体 + 确认 0 残留 → 全量 GREEN。
- **最强早期信号**：`cargo test --lib`（runtime 单测，[[xtask-test-excludes-cargo-test]]）+ boxed-int 反射 golden。
- **格式中立自证**：编译器不动 → self-host 不动点应逐字节复现（gen1==gen2）；zbc/zpkg minor 不变。

## Testing Strategy

- **cargo `--lib`**：boxed-prim alloc/unbox/引用身份/GetType/equality 单测。
- **golden e2e**：`object o = 5; Console.WriteLine(o.GetType().Name)` → "Int32"；`o.ToString()` → "5"；
  `ReferenceEquals(box1, box2)` → false；反射 GetValue/SetValue on boxed int；boxed→unbox roundtrip。
- **self-host 不动点** + 全量 `xtask test`（含 cargo test）0 失败 + 无格式 bump。
