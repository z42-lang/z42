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

## 与逃逸分析 / packed 数组的关系

- struct 恒内联，**不走** `ObjNew`→堆/`StackObject` arena（Decision θ）；逃逸 arena 是**引用类型**的
  分配优化，struct 内联是**值类型**的语言语义——两套机制。
- 字节 blob 地基与 [packed-primitive-arrays] 的字节 `ArrayBacking` 收敛（P3 的 `struct[]` 字节 backing）。

## 收敛面与延后（A-use 首版）

- ✅ 局部多字段扁平 struct：构造 / 复制 / 字段 get·set / `this` 字段 / 嵌套局部 lvalue（叶子直写）。
- ⏳ Deferred：**嵌套 struct 字段**（`Line{a:Point}`，区间复制）、**struct 传参 copy-in**、
  **struct 返回值**（帧生命周期 ABI）、**单标量叶子 struct 塌缩**（`GCHandle` 保持现有标量模型=Phase B）、
  **对象内联 struct 字段 / `struct[]`**（P3）、**跨包布局元数据 / 装箱 / 反射**（P4）、**JIT 值路径**（P5）。
