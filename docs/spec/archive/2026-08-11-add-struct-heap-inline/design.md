# Design: struct 裸字节内联进堆对象字段 + `struct[]` 字节 backing（P3b）

> 状态：🟢 APPROVED（2026-08-11）。User 审批裁决 **D1 = D1-a**（基元内联 + 引用侧表）+ **路线 α**（复用 0xC0–0xC3）+ **一个 PR 全做**。
> 事实校正后 D1-a 取代字面 R1（D1-b）：密度/FFI payoff 全在基元打包，引用叶子侧表无损且内存安全。以下 §1 保留决策全貌备查。

---

## 0. 目标与缺口（已探明）

补完 struct 值语义最后一个洞：struct 存进**堆对象字段** `class C { Point pt; }` 与 **`Point[]`**。

**当前行为（缺口，`ExprEmitter.z42:775` `_isBlobStruct` 判的是 target/容器类型，从不判字段类型）**：
- `class C { Point pt; }` 的 `c.pt = p` 走普通 `FieldSetInstr(obj, "pt", val)`（`ExprEmitter.z42:700`），把一个 `Value` 句柄存进 `obj.slots[i]`。运行时该 `Value` 是**帧作用域 `Value::StructRef{idx,frame_id}`**（`is_heap_ref=false`，`types.rs:945`）→ 帧退出 arena 截断 → **悬垂 use-after-free**。
- `Point[]` 走 `ArrayBacking::Boxed(Vec<Value>)`（`types.rs:543`，struct 落 reference array），每元素同样是帧作用域句柄。

P3b 让 struct 字节**内联**进堆对象/数组，兑现密度 + FFI 零 marshaling + 零 per-field 堆分配。

---

## 1. Decision D1（核心，须 User 审批）：引用叶子的内联表示

### 已有的安全地基（两处，均已合并 main）

| 载体 | 基元叶子 | 引用叶子 | GC |
|------|---------|---------|-----|
| arena `StructSlot`（帧作用域，`struct_arena.rs:44`） | `bytes: Box<[u8]>` 打包 | `refs: Box<[Value]>` **侧表** | `scan_roots` 逐 `refs` visit（根扫） |
| `Value::BoxedStruct`（脱帧堆，PR2a） | `bytes` | `refs: Box<[Value]>` **侧表** | `scan_object_refs`/`trace_children` 逐 `b.refs` visit |

**两处引用叶子都是真 `Value`**，从不裸字节。这是 A-support/PR2a 刻意的内存安全决定（design 里「引用叶子=安全侧表非裸字节 blob，Rust 内存安全，γ 密度靠基元字节保留」）。

### User 选的 R1（裸字节内联）与其健全性障碍

User 的 preview：`ScriptObject{ slots, struct_bytes: Box<[u8]> }`（基元 + **16B 句柄都内联**）+ `TypeDesc{ struct_ref_offsets }` + GC「按 offset 从 bytes **unsafe 重建 &Value**」。

**事实校正（CLAUDE.md 事实校正责任）——这一步在 Rust 里不平凡，且部分不健全**：

1. **GC 访问协议是 `visitor(&Value)`**（`arc_heap.rs:2005`、`types.rs:1000`）。`Value` 是一个**带判别式的 enum，远大于 16B**（含 I64/Bool/Object(GcRef)/Str(Arc<str>)/… 多变体）。把「只有 16B 句柄载荷」的字节还原成一个**完整 `&Value`** 递给 visitor——需在扫描时**在栈上物化临时 `Value`** 再传 `&temp`。
2. **`Arc<str>` 裸存字节绕过 Rust 所有权**：inline 的 string 叶子若作 raw 16B（胖指针）存 `Box<[u8]>`，则 `StructCopy`/字段写/GC 都要手工 `ManuallyDrop` / `Arc::from_raw` / `mem::forget` 管引用计数，漏一处即 double-free 或泄漏。
3. **`GcRef` 代际有效性**：裸字节存的 GcRef 在物化成 `Value::Object` 前无 Rust 生命周期护栏，unsafe 面扩大。

**结论**：**基元叶子**裸字节内联是安全且直接的（纯 memcpy，FFI 对纯基元 struct 零 marshaling ✓）；**引用叶子**裸 16B 内联才是 unsafe/易错的那部分，且它**不与现有 `Box<[Value]>` / `visitor(&Value)` 协议平凡组合**。

### 三个可落地变体（请 User 在审批口选一）

| 变体 | 基元 | 引用叶子 | 密度 | 安全 | 与已有一致 | Scope |
|------|------|---------|------|------|-----------|-------|
| **D1-a（推荐）** 基元裸内联 + 引用侧表 | `struct_bytes: Box<[u8]>` 内联进对象 | 对象带 `struct_refs: Box<[Value]>` 侧表（按已持久化 ref 位图定位） | 高（基元打包；引用 16B 句柄本就是侧表一格，无损） | ✅ 复用 `visitor(&Value)`：scan 逐 `struct_refs` visit（同 BoxedStruct.refs）；写引用叶子=写 Value 槽→现成 `write_barrier_field` | ✅ 与 arena/BoxedStruct 同构（都是 bytes+refs） | 中 |
| **D1-b** 全裸内联（User 字面 R1） | 内联 | **16B 句柄裸内联字节区**，TypeDesc 带 `(off,kind)` 表 | 最高（真 C# 布局） | ⚠️ unsafe：GC 按位图物化临时 Value + 手工 Arc 计数 | ✗ 偏离侧表决定，新增 unsafe 面 | 大 |
| **D1-c** 只装箱 | 每 struct 字段一次堆 alloc 存 `Value::BoxedStruct` 进 slot | — | 无 | ✅ 全复用现成 BoxedStruct 臂 | ✅ | 小（但无 payoff） |

**我的推荐 = D1-a**。理由：
- **拿到 User 要的密度与 FFI**：基元真打包进对象字节区（`class C{ int x; short y; bool z; }` 的 struct 字段按 4/2/1B 内联，非每字段一个 8B Value 槽）；纯基元 struct 可直喂 native（FFI 零 marshaling）——**这正是 R1 的核心 payoff，D1-a 全保留**。
- **不为「引用叶子也裸内联」这一段付 unsafe 税**：引用叶子无论如何都是 16B 托管句柄，放侧表 `Box<[Value]>` 相对放字节区**无密度损失**，却换回内存安全 + 与 arena/BoxedStruct 完全同构（`StructCopy` arena↔堆是 `bytes` memcpy + `refs` 逐 `Value` clone，无转码）。
- D1-b 的额外密度收益**仅在引用叶子那 16B**，代价是永久 unsafe + GC 物化临时 + 手工 Arc——**投入产出比差**。若将来 B-radical 要极致密度可再上 D1-b。

> **若 User 坚持 D1-b**：我照做，但 design 会补一节 unsafe 契约（inline handle 的 `#[repr]` 对齐、`ManuallyDrop` 边界、GC 物化 `ManuallyDrop<Value>` 不 drop 的规约、Miri 门），且 Scope/风险显著上升、JIT 值路径更难。

### Open Question（proposal 已提，一并在此裁决）：表示分裂

D1-a/b 都让**堆内联** struct 与 **arena** struct 表示一致（a=都侧表；b 则 arena 仍侧表、堆裸字节=**分裂**）。选 D1-a → 天然无分裂；选 D1-b → arena（侧表）↔堆（裸字节）边界每次复制须逐叶子转码。**这本身也是选 D1-a 的一条理由。**

---

## 2. 对象内联布局（以 D1-a 为基线；D1-b 差异标注）

### ScriptObject 扩展

```rust
pub struct ScriptObject {
    pub type_desc: Arc<TypeDesc>,
    pub slots: Box<[Value]>,          // 非内联字段照旧
    pub struct_bytes: Box<[u8]>,      // 【新】所有内联 struct 字段的打包字节区（无内联字段=空）
    pub struct_refs: Box<[Value]>,    // 【新，D1-a】内联 struct 字段的引用叶子侧表（D1-b 无此，改字节内联）
    pub native: NativeData,
    pub type_args: Box<[String]>,
}
```

- **字段分类**：类的每个字段，若其类型 `IsBlobStruct` → **内联字段**（不占 `slots`，占 `struct_bytes` 一段 `[field_byte_off, +struct_size)` + `struct_refs` 若干格）；否则 **普通字段**（占 `slots` 一格，照旧）。
- **对象布局元数据（TypeDesc）**：新增每字段「是否内联 + 内联则 byte offset + struct type name（→查其 `struct_layout` 得 size/ref 位图）」。普通字段仍走 `field_index→slot`。
- **alloc**：`struct_bytes = vec![0u8; total_inline_bytes]`（零初始化=struct 默认值，与 C# 一致）、`struct_refs = vec![Value::Null; total_inline_refs]`（D1-a）。普通 `slots` 初值不变（`exec_object.rs:66` 逻辑保留，仅遍历「普通字段」子集）。

### 字段访问指令

内联 struct 字段的叶子访问 = 现有 `StructFieldGetPrim/SetPrim` 的**堆对象 base 版**。当前这四条指令（StructAlloc/Copy/FieldGetPrim/SetPrim，opcode 0xC0–0xC3）的 base 是 arena `StructRef`；P3b 让它们**也能以「堆对象 + 对象内 byte offset」为 base**。两条路线：

- **路线 α（推荐，最小格式面）**：不加新 opcode。`c.pt.x` codegen 发 `FieldGet(obj,"pt")` 得内联 struct 的**地址句柄**（新 `Value::StructRef` 变体：base=堆对象、off=pt 的对象内偏移），后续 `.x` 复用 arena 版 `StructFieldGetPrim`。即扩 `StructRef` 让它能指向**堆对象字节区**而非只 arena。GC/生命周期：这种 StructRef 指向堆对象，须让对象存活（`is_heap_ref` 处理）。
- **路线 β**：新增 4 条「堆 base」指令（opcode 0xC4–0xC7），object+field-name+leaf-offset。格式面更大。

D1 定后细化选 α/β；倾向 α（复用指令、格式仅动类描述符）。

---

## 3. `struct[]` 字节 backing

`ArrayBacking` 加变体（`types.rs:489`）：

```rust
StructBytes { elem_size: usize, bytes: Vec<u8>, refs: Vec<Value>, layout: Arc<StructTypeLayout> }  // D1-a
```

- `pack_backing`（`types.rs:526`）：元素类型 `IsBlobStruct` → `StructBytes`（非落 `Boxed`）。`len` 元素 = `len*elem_size` 字节 + `len*ref_count` refs。
- `arr[i]` = 内联 struct 地址句柄（base=array、off=`i*elem_size`）；`arr[i].x=v` 叶子直写（3a 原地可变）。
- GC：`boxed_slice()`（`types.rs:609`）对 `StructBytes` 返回 `None`（不当引用数组扫），改由 array 的 scan 走 `refs` 侧表（D1-a）/按位图物化（D1-b）。**收敛 [[packed-primitive-arrays]] 的 inline struct[]**。

---

## 4. GC：扫描 + 写屏障（本阶段核心难点，即 memory 标记的设计分叉）

**扫描（mark 阶段追踪可达性）**——`scan_object_refs`/`trace_children` 的 `Value::Object`/`Value::Array` 分支扩展：
- **D1-a**：visit `obj.struct_refs` 每格（与现有 `BoxedStruct.refs` 分支一行同构，`arc_heap.rs:2005`）。零 unsafe。
- **D1-b**：按 TypeDesc 的 `(off,kind)` 表，从 `struct_bytes` 物化临时 `Value`（`kind=GcRef`→`ManuallyDrop<Value::Object>`；`kind=ArcString`→临时 `&str` 视图）递给 visitor，扫完不 drop。

**写屏障（并发/分代模式正确性）**——`write_barrier_field`（`arc_heap.rs:1836`，STW 默认 no-op，仅 Concurrent/Generational 生效）：
- **D1-a**：写内联引用叶子 = 写 `obj.struct_refs[k]`（一个 Value 槽）→ 直接复用 `write_barrier_field(owner, k, new)`，`is_heap_ref` 过滤基元照旧。**无新屏障机制**。
- **D1-b**：写内联字节区的引用叶子 = 按 offset 手工触发屏障（新 `write_barrier_inline_struct_field(owner, byte_off, kind, new)`），且要处理旧值 Arc drop。

> **这就是 memory 里「①scan 递归进内联区 vs ②写叶子触发屏障」的分叉**：正确的并发 GC **两者都要**（scan 供 mark，barrier 供并发 SATB/分代 card）。D1-a 让两者都**平凡复用现有 Value 侧表机制**；D1-b 两者都要新写 unsafe 路径。**这是 D1-a 相对 D1-b 最大的工程简化。**

---

## 5. 编译器 codegen 改动

- **字段 get/set 翻转判据扩展**（`ExprEmitter.z42:700/753`）：`obj.f` 当 **`f` 的字段类型 `IsBlobStruct`**（现在只判 target/owner 类型，`:775`）→ 走内联 struct 字段访问（发地址句柄 + 叶子 prim 访问，路线 α），而非普通 `FieldGet/FieldSetInstr` 存 `Value`。
- **整体 struct 字段写**（`c.pt = otherPoint`）：两侧 blob → `StructCopy`（base 一侧是堆对象内偏移）。复用 `_copyRegion` 逐叶子（`ExprEmitter.z42:651` 已有裸 owner 版）。
- **数组元素**：`arr[i]` / `arr[i].x=v` 发数组 base 的地址句柄 + 叶子访问。
- **无内联字段的类**：codegen 逐字节不变（所有字段仍走 slots）→ 自举/现有 golden 零回归。z42c/stdlib 现零生产「含 struct 字段的 class」→ 翻转纯增量。

---

## 6. 格式 bump（zbc 1.31→1.32 / zpkg 0.36→0.37）

版本坐标（version-bumping.md 表）：`ZbcFormat.z42:8`（Minor 31→32）、`zbc_reader.rs` `ZBC_VERSION_MINOR`、`ZpkgWriter.z42` `ZpkgWriterZ.Minor`（36→37）、`zbc_reader.rs` `ZPKG_VERSION_MINOR`。

**新增 wire 内容 = 类描述符的「内联字段表」**（struct 自身的 ref 位图 zbc1.31 已持久化，`ZbcWriter.z42:346` / `zbc_reader.rs:568`，**直接复用**）：

TYPE section 每个 class 记录，在现有字段列表上，为每字段补「是否内联 struct + 内联 byte offset」，并加对象 `struct_bytes` 总大小 / `struct_refs` 总数（可运行时由内联字段推导，若推导则不入 wire——D1 后定最小 wire 面）。精确字节布局落 `specs/format-delta.md`。

version-bumping 6/9 步 + fixture 重生（zbc-format 6 个 `xtask build test` 自动；zpkg-format 4 个手工 `z42c build` 重生，含 indexed/sym-only 已探明 recipe，见 [[struct-value-semantics-program]]）+ golden hex 重截 + changelog（zbc.md/zpkg.md）。

**bootstrap 两阶段 nightly**：本阶段 z42c/stdlib **不使用**含 struct 字段的 class（zero-production 已确认）→ writer 加「内联字段表」但当前源码不 emit 它 → 上一 nightly z42c 能编当前源 → `xtask test bootstrap` gate 应绿。格式 bump 的 cold 路径靠 CI 两代自举（macOS 本地环境墙，见 [[escape-stack-format-bump-ci-learnings]]）。

---

## 7. 反射 / 装箱一致

- 内联 struct 字段读出后若装箱进 `object` → 复用 PR2a `__box_struct`（从对象内偏移拷进 `BoxedStruct`）。
- `GetType`/`is`/`as` 走已有 BoxedStruct/StructRef 臂（PR2a）。
- 跨包完整反射内联布局 = Deferred（P4）；本阶段保证同包/已加载。

---

## 8. 验证策略

- `cargo test --lib`（新增内联 alloc/scan/copy/写屏障单测 + 值语义：`c.pt` 独立性、`arr[i].x` 原地写、帧退出后堆内联叶子存活）。
- 并发 GC 模式 golden（`Z42_GC_MODE=concurrent`）压内联引用叶子的 mark/barrier 正确（防漏标过早回收）。
- `xtask test` **不传 Z42_HOME**（血泪教训）+ self-host 5/5 gen1==gen2。
- e2e golden：`src/tests/types/struct_heap_inline.z42`（class struct 字段 r/w + 独立复制 + string 叶子 + `Point[]` 元素原地可变 + 装箱往返）。
- 格式 bump CI 两代自举吸收；`bootstrap` gate 绿证无越界。

---

## 9. 阶梯位置

P3b 完成后 struct 值类型语义**功能面基本闭合**（局部/字段/嵌套/==/装箱/对象协议/容器/堆内联/数组）。之后：**P4** 跨包内联布局 + 完整反射；**P5** JIT 值路径（现全 bail→interp）；**B-radical** 消灭 Z42PrimType + 单叶子塌缩（若届时要极致密度可上 D1-b）。
