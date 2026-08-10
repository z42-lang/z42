# struct 值语义（内联字节 blob）

> 状态：A-use 落地（2026-08-09，zbc 1.31 / zpkg 0.36）。本页讲**多字段 struct 的真值语义**
> 如何在编译器 + 运行时实现。程序全景（选项 B / B-radical 统一值类型 / 分阶段）见
> `docs/spec/changes/add-struct-value-semantics/`。

## 目标

z42 的 `struct` 是 **C# 真值类型**：赋值 / 传参 / 存容器 = **字段级复制**，不是共享堆对象引用。

```z42
struct Point { public int x; public int y; public Point(int x,int y){this.x=x;this.y=y;} }
var a = new Point(1, 2);
var b = a;      // 值复制
b.x = 99;       // 只改 b
// a 仍是 (1,2)  —— 引用语义下 a.x 会跟着变成 99
```

现状（A-use 前）：`struct == class == Value::Object(GcRef)`，`b=a` 克隆句柄 → 串味。A-use 把
**多字段复合 struct** 翻转为内联字节 blob 值语义。

## 布局：字节精确

编译器 `StructLayout`（`z42c.semantics`）为每个 struct 类型算**字节扁平布局**：每个直接字段的
`(byte_offset, size, kind)`、类型总 `size/align`，以及**带种类的引用叶子表**（引用位图）。

- 基元字节精确：`i8/u8/bool=1`、`i16/u16=2`、`i32/u32/f32/char=4`、`i64/u64/f64=8`（`char`=4B
  Unicode 标量）。
- 引用叶子（`string` / object / array 字段）= 16B 托管句柄，进**引用位图**（种类 ArcString / GcRef）。
- `Point{x:int,y:int}` → `size=8, {x:@0, y:@4}`，引用位图空。

## 运行时：per-context 字节 arena + 侧表引用叶子

未装箱 struct 值 = per-`VmContext` **字节 arena**（`interp/struct_arena.rs`）里的一段 blob；寄存器持
`Value::StructRef{idx, frame_id}` 句柄（仿 `StackObject`，LIFO 随帧退出截断，`frame_id` staleness 守卫）。

一个 blob（`StructSlot`）= **两部分**：

| 部分 | 存什么 | 为什么分开 |
|------|--------|-----------|
| `bytes: Box<[u8]>` | 基元叶子，**字节打包**在布局偏移处 | γ 密度：逼近 C# 内存密度 |
| `refs: Box<[Value]>` | 引用叶子（`string`/object/array），作**真 `Value`** | Rust 内存安全：`Arc<str>`/`GcRef` 的裸字节写进 `[u8]` 会漏引用计数（泄漏/double-free）、moving GC 也无法改写；侧表由 `Value` 的 clone/drop 正确托管 |

`refs` 按引用位图（`ref_offsets`）排序；字段访问按 byte offset 映射到 `refs` 槽。

### 四条 IR 指令（zbc opcode 0xC0–0xC3）

| 指令 | 语义 |
|------|------|
| `StructAlloc dst, type_name, size` | 在 arena 分配零初始化 blob，`dst` = StructRef 句柄 |
| `StructCopy dst, src, size` | 复制 blob：字节 memcpy + 逐引用叶子 `Value::clone`（值语义） |
| `StructFieldGetPrim dst, base, byte_off, kind` | 读叶子：基元走字节 codec、引用走 `refs` 侧表 |
| `StructFieldSetPrim base, byte_off, kind, val` | 原地写叶子（3a lvalue），同上分流 |

`kind` 是运行期 `TypeTag`（`TAG_I32`/`TAG_STR`/…），给字节宽 + 解码 / 或标识引用叶子。字段 byte
offset / size 由编译期烘焙为**立即数**，运行时无需查表。

### GC：arena 是根，P1 无写屏障

字节 arena 每次采集都作 **GC 根**整体重扫（`scan_roots` 遍历每个 blob 的 `refs`，与 `stack_alloc`
arena 同）→ blob 内引用叶子恒被重标记。因此**写引用进 arena blob 不需写屏障**——写屏障只对「引用写进
**堆对象**」必需（堆对象不作根重扫），即 struct 内联进对象/数组的 **P3**，非本阶段的 P1 局部 struct。

## codegen 翻转（A-use）

`z42c` 的 `ExprEmitter`/`FunctionEmitter` 对 **blob 值 struct**（`StructLayout.IsBlobStruct`：多字段
且各字段非嵌套 struct）发射上述指令：

- `new P(...)` → `StructAlloc` 句柄 + `call ctor(句柄, args)`；ctor body 的 `this.f = a` 因所属类是
  blob struct 翻转为 `StructFieldSetPrim(句柄, offset, tag, a)`，**原地**填 blob（句柄携创建帧
  `frame_id`，跨 ctor 子帧仍解同一 arena 槽）。
- `P b = a`（非 `new`）→ `StructAlloc b` + `StructCopy(b, a)`；`P b = new P(...)` 直接别名 fresh 句柄。
- `b.x = v` → `StructFieldSetPrim`；`a.x` 读 → `StructFieldGetPrim`。
- `this.x` / 裸字段（struct 方法/ctor 内）→ 同上（`this`=reg0 句柄）。

**优化器完整性**：4 条指令的 def/use 必须录入 `IrOptInfo`（`DstId`/`AddReads`/`ReplaceReads`/`SetDst`）
+ 逃逸分析汇点表——漏 `StructFieldSetPrim` 的 `Val` 读 → DCE 误删喂值的 `const`（实测踩坑）。struct
方法暂不入 inline 允许集（`_isInlinable`），保守不内联。

## 嵌套 struct 字段（add-struct-nested-fields）

`struct Line { P a; P b; }`——字段本身是 struct。布局早已递归展平（嵌套 P 的叶子按偏移平移并入 Line
的字节区间 + 引用位图），故 `line.a.x` 的字节地址是**编译期可算的累积 offset**：`off(Line,a)+off(P,x)`。

**准入**：`IsBlobStruct` 去掉"含嵌套 struct 字段即拒"的旧门，改为接受（仍要求 `FieldCount>=2` 且
`Size>0`——后者兜住自引用 struct 的空布局，见下）。

**叶子读写（3a 原地）**：`line.a.x` / `line.a.x = 3` 沿成员链**累积 byte offset**，对根 blob 句柄发射
**单条**现有 `StructFieldGetPrim` / `StructFieldSetPrim`——无新指令、无格式 bump。链根解析两遍互补、
不重复发射：`_structChainRoot` 只 Emit 根一次（局部 / `this` reg0 / 拥有者裸 struct 字段），
`_structChainOffset` 纯查布局表累加偏移。扁平单层 `a.x` 是其退化情形（offset=0），codegen 逐字节不变。

**整字段复制**：`P p = line.a`（读出）/ `line.a = q`（写入）= 对子 struct 的叶子**逐叶子分解复制**
（递归到真叶子；基元走字节 codec、引用叶子走侧表 `get_ref`/`set_ref`），复用现有 Get/SetPrim，
不引入区间复制指令。值语义：`p` 得独立副本，改 `p.x` 不动 `line.a.x`。

**自引用兜底**：`struct Node { Node next; }` = 无限大小（C# `CS0523`）。`LayoutOf` 的 `_inProgress`
环检测置 `ErrorType` 并返回空布局（`Size==0`）→ `IsBlobStruct` 的 `Size==0` 门拒之 → 退化引用语义
（与今日一致、不崩）。显式 `E0438` 诊断留 follow-up。

## 与逃逸分析 / packed 数组的关系

- struct 恒内联，**不走** `ObjNew`→堆/`StackObject` arena（Decision θ）；逃逸 arena 是**引用类型**的
  分配优化，struct 内联是**值类型**的语言语义——两套机制。
- 字节 blob 地基与 [packed-primitive-arrays] 的字节 `ArrayBacking` 收敛（P3 的 `struct[]` 字节 backing）。

## 收敛面与延后

- ✅ 局部多字段扁平 struct：构造 / 复制 / 字段 get·set / `this` 字段 / 传参 copy-in / 返回值 sret（A-use）。
- ✅ **嵌套 struct 字段**（`Line{a:P}`）：累积-offset 叶子读写（3a）+ 整字段逐叶子复制（add-struct-nested-fields）。
- ⏳ Deferred：**`struct==` 值相等**（逐叶子比较，需新指令）、**单标量叶子 struct 塌缩**（`GCHandle` 保持
  现有标量模型=Phase B）、**对象内联 struct 字段 / `struct[]`**（P3）、**跨包布局元数据 / 装箱 / 反射**（P4）、
  **JIT 值路径**（P5）、**E0438 自引用诊断**（当前 `Size==0` 兜底防崩）。
