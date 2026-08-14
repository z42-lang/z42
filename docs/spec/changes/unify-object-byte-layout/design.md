# Design: 统一 struct/class 内存布局 + 引用压 8B（路 A）

> 终点 = C# 完全等价（User 裁决）；非移动 GC（路 A 标记指针），不做移动/分代 GC。

## Architecture

```
                    ┌─────────────────────────────────────────────┐
      现状（P3b）    │ ScriptObject                                 │
                    │  slots: Box<[Value]>   ← 直接字段(基元/引用) 24B/格
                    │  struct_bytes: Box<[u8]> ← 内联 struct 基元叶子
                    │  struct_refs: Box<[Value]] ← 内联 struct 引用叶子
                    └─────────────────────────────────────────────┘
                                     │  统一
                                     ▼
                    ┌─────────────────────────────────────────────┐
      终点          │ ScriptObject                                 │
                    │  bytes: Box<[u8]>   ← 全部字段的 C 顺序字节布局:
                    │      · 基元 = 自然宽度内联                    │
                    │      · 引用 = 8B 裸指针内联(GcRef/Str 细指针)  │
                    │      · 内联 struct = 扁平嵌入                 │
                    │  (无 slots；无 refs 侧表——引用直接在 bytes 里) │
                    │  ref_bitmap(来自 TypeDesc): 哪些 byte offset 是引用
                    └─────────────────────────────────────────────┘

   GcRef 16B → 8B:  [ 48位地址 | 16位窄generation ]  deref: ptr & 0x0000_FFFF_FFFF_FFFF
   Value  24B → 16B: repr(C,u8) tag(8) + payload(≤8) 
   String 16B → 8B: Arc<StrHeader{ len:usize, bytes:[u8] }> 细指针
```

**一句话**：把 P3b 已经为「内联 struct 字段」验证过的「字节区 + 引用位图 + byte-offset 访问 + zbc 元数据 + GC 扫描 + 写屏障 + JIT 桥」范式，推广到**对象全部直接字段**，同时把引用叶子从 `Value` 侧表**内联成 8B 裸指针**（借标记指针保住 use-after-free 安全 + 细指针字符串）。

## Decisions

### Decision 1: 对象字段存储 —— 统一 byte-offset，删 `slots`
**问题**：class 直接字段现在是 `slots: Box<[Value]>`（24B/格），与 struct 的字节区两套模型。
**决定**：class 用**单一 `bytes: Box<[u8]>`** 承载全部直接字段的 C 顺序布局；基元自然宽度、引用 8B 内联、内联 struct 扁平嵌入。`slots` 删除。字段访问一律 byte-offset（复用 `StructFieldGetPrim/SetPrim` 的对象基址路线，编译期烘焙 offset）。
**理由**：与 struct 收敛为一套；基元密度立得；P3b 已证明该路线可行。`P3b` 的 `struct_refs` Value 侧表在终点被**内联 8B 指针 + 位图**取代（见 D5）。

### Decision 2: 8B 引用 —— 路 A 标记指针（保非移动 GC）
**问题**：`GcRef` = 指针8 + generation4 + pad = 16B。要 8B 且保住 ABA/use-after-free 护栏。
**选项**：A 标记指针（窄 generation 塞进 48 位地址高 16 位，非移动 GC 不变，deref 一次 mask）；B 移动 GC 弃 generation（改动最大，= object-abi §6）。
**决定**：**路 A**。终点 C 不要求移动 GC；路 A 改动局限在 `GcRef` 表示 + deref mask + alloc 时写 generation 到高位，region/非移动堆不动。
**理由**：以最小 GC 风险达成 8B。移动 GC 作为独立后续（§6/P3）。
**权衡/风险**：generation 16 位 → ABA 窗口变窄（Open Question，需评估当前回绕速度）；与 ARM MTE/PAC、ASAN top-byte 交互需在 CI 目标平台验证。

### Decision 3: 字符串 8B —— 细指针 `StrHeader`
**问题**：`Value::Str = Arc<str>` 胖指针 16B。
**决定**：改 `Arc<StrHeader{ len:usize, bytes:[u8] }>` 细指针（8B），长度进堆对象头（CLR/JVM 模型）。**保留 Arc 引用计数**（不强制此变更内把字符串纳入 tracing GC —— 那是 §5 的更大议题；细指针 + Arc 即可达成 8B）。
**理由**：达成 8B payload 的必要条件；与 §5「字符串改 GC」同向但不绑定其全部。
**权衡**：取 `Length` 多一次 deref（Open Question benchmark）；所有 `Arc<str>` 用点迁移（机械但面广）。

### Decision 4: `Value` enum 24B→16B
**问题**：最大 payload 从 16B（Arc<str>/GcRef）降到 8B 后，`Value` 可 24→16B。
**决定**：`Value` = `#[repr(C,u8)]` tag(8 对齐) + payload(8) = **16B**。更新 `value_layout` 断言、JIT `STRIDE 24→16 / PAYLOAD 8`。
**理由**：寄存器 / 数组 boxed 元素 / 任何仍以 Value 存的地方省 33%，cache 收益。
**非目标**：不做 NaN-box（tag 也进 payload 到 8B）—— 复杂度不值,16B 已达主要收益。

### Decision 5: GC 精确扫描 —— 对象级引用位图
**问题**：现在 GC 逐 slot 看 Value tag（`trace_children`/`scan_object_refs`）；字段内联成裸 8B 指针后 slot 没了。
**决定**：`TypeDesc` 带**对象级引用位图**（`ref_offsets`+`ref_kinds`，复用 `StructTypeLayout`，已为内联 struct 存在，扩到全字段）。GC 按位图读每个引用 offset 的 8B 裸指针，按 kind（GcRef-object / GcRef-array / Str）重建句柄并 mark。
**理由**：精确、无需逐字段 tag 分支；这是「裸指针内联」内存安全的另一半（写对了位图才能正确扫）。
**风险**：位图/offset 错 → 扫错内存 = UB。需 D1-a 已有的三层校验思路 + golden + Miri/ASAN。

### Decision 6: object-abi.md §3 修订（解决规范冲突）
**问题**：object-abi §3 明确「class `slots: Value[]`」，与本变更冲突（CLAUDE.md 规范冲突检测要求先裁决）。
**决定**：User 已裁决走统一（选项 3）。§3 的「普通 ref 对象 payload = `slots: Value[]`，逐 slot 看 tag」修订为「payload = `bytes` C 顺序布局，引用 8B 内联，GC 按对象级引用位图扫」。§2.1 从 Deferred 提为**已采纳（路 A）**。§5 字符串细指针记为本变更落地。
**同步**：修订随本变更落 `object-abi.md`（Scope 内 MODIFY），归档时对齐日期刷新。

### Decision 7: 交付切分（GREEN 铁律，终点仍 C）
**问题**：单巨改无法小步全绿；workflow 阶段 8 禁止未全绿 commit。
**决定**：终点锁死 C，实现拆为**内部阶段 / 多 PR**，每 PR 独立 GREEN + rebase：
1. **PR-1 布局元数据（行为不变）**：编译器 `StructLayout` 扩为对象全字段布局 + writer/reader 发/读该表；runtime **暂不切存储**（仍 slots），只多带一份 `TypeDesc` 布局。格式 bump。可全绿（老路径不动）。
2. **PR-2 runtime 切字节存储**：`ScriptObject` 删 slots → `bytes`；FieldGet/Set/IC/反射/JIT 改 byte-offset；引用**仍 16B**（暂存 bytes 里占 16B 或 refs 侧表）；GC 位图扫。达成「struct/class 统一 + 基元压缩」。全绿。
3. **PR-3 引用 8B 标记指针**：`GcRef` 16→8B（路 A）；对象布局引用 offset 16→8B；GC 按 8B 读。全绿。
4. **PR-4 字符串细指针**：`Arc<str>`→`StrHeader`；String payload 8B。全绿。
5. **PR-5 Value 16B + JIT 收尾**：Value payload 收窄、`STRIDE 16`、value_layout 断言。全绿。
**理由**：每步可回退、可对账（自举字节不动点 / golden）；PR-1 先把格式 bump 单独消化，降低后续耦合。

## Implementation Notes

- **复用点**（勿重造）：`StructTypeLayout` / `StructFieldGetPrim/SetPrim` / `inline_region_sizes` / `write_barrier_field` / zbc 1.32 表结构 / `ClassDescBuilder` 布局合成 / `ExprEmitter._structChainOffset` / JIT `jit_struct_field_*` helper 桥。
- **双存储清理**：现 struct 字段占「死 Null slot + struct_bytes 窗口」，PR-2 删 slots 时自然消除。
- **继承**：基类字段在前、子类追加的 offset 稳定性（object-abi §3）—— 对象布局须保持基→派生 offset 单调，复用现 field_index 继承规则。
- **JIT 硬编码**：`translate.rs` `STRIDE=24/PAYLOAD=8`（`1335-1336` 等）+ 数组元素 24 → 全改；方案 B `jit_obj_field_slot` slot 指针 → byte-offset 基址。
- **ABI 断言**：`abi_layout_tests.rs::value_is_16_bytes` 已是 16（Z42Value 边界类型），内部 `Value` 的 value_layout 断言另需更新。

## Testing Strategy

- **单元**：`StructLayout` 对象全字段布局（基元/引用/内联 struct/继承）offset & 位图；`GcRef` 标记指针 pack/unpack + generation 校验；`StrHeader` len/bytes。
- **Golden（e2e）**：class 基元字段值语义、引用字段读写、内联 struct、跨包对象、反射 Get/SetValue、GC 存活/回收（含 8B 引用 + 字符串）、`--mode jit` 同结果。
- **GC 安全**：Miri / ASAN 跑对象扫描（位图正确性）；ABA 压力测试评估窄 generation。
- **自举**：`xtask test compiler` 5/5 byte-identical；`xtask test bootstrap` 无越界。
- **格式**：`xtask build test` 重生 zbc/zpkg fixture；`cargo test zbc_compat / lazy_loader`。
- **完整 GREEN**：每 PR `xtask test` 全 stage。

## PR-2 Implementation Notes（2026-08-14，User 裁决 Option B = 运行时组合）

落地 D1（删 `slots`，统一到单 `bytes` 区）的具体机制，把 P3b 已验证的「对象基址字节访问 +
`ref_index` 侧表」范式（`exec_struct::struct_field_get_val` / `struct_field_set_val` 的
`Value::Object` 臂）从「仅内联 struct 字段」推广到**对象全部直接字段**。

### D8: 引用暂存侧表（不内联进 bytes）—— 8B-baked offset 强制
PR-1 的 `ObjectLayoutDesc` 按 **8B 引用宽度**记 offset。PR-2 引用仍 16B（`Value`），**无法**内联进
bytes 的 8B 槽（会错位后续字段）。故 PR-2：`bytes: Box<[u8]>` 承载**全部基元叶子**（含内联 struct
基元叶子），引用字段的 8B 槽是**空洞**（dead，PR-3 填 8B 指针）；`refs: Box<[Value]>` 承载**全部
引用叶子**（含内联 struct 引用叶子），按 composed 引用位图序。删 `slots`/`struct_bytes`/`struct_refs`
（三区收敛成 bytes+refs）。ref-heavy 对象 PR-2 暂多花 8B 洞/引用，PR-3 消除。这是 D7 PR-2「暂存
bytes 里占 16B 或 refs 侧表」中**侧表**分支——16B 内联分支被 8B-baked offset 证伪。

### D9: 继承组合由运行时 loader 做（Option B）
zbc `object_layout` **保持 PR-1 的 own-only**（本类字段、offset 从 0、无 base-shift；不改写入值、
无格式变更）。运行时 loader 组合 `composed = base.composed ++ own`（镜像现有 `fields` =
base.fields++own_fields 的组合，见 `loader::try_fixup_inheritance`）：base 字段在前、own 字段按
对齐追加（base composed size 起）。composed 布局产出：
- `total_size`（bytes 长度）；
- 每字段 name→(composed offset, size, kind)（对齐 `fields`/`field_index` 的 slot 序，供 FieldGet 按名解析）；
- composed 引用位图（`ref_offsets`+`ref_kinds`，含内联 struct 内部叶子，供 `ref_index` 映射 + PR-3 GC 扫）。

**编译器侧对称组合（风险点，须字节一致）**：内联 struct **叶子** offset 是编译期烘焙的
（`ExprEmitter._structChainOffset`，非运行时解析），故编译器烘焙时也必须算 base-shift = base composed
size。编译器的 `StructLayout._computeObjFields` 现只算 own（offset 从 0），PR-2 需让内联 struct 字段的
**根对象相对 offset** 取 composed（base-shift + own）。两处组合（loader / 编译器）算法必须逐字节一致
（base-first + 同一对齐规则），由 `xtask test compiler` 5/5 byte-identical 兜底校验分歧。

### D10: 字段访问分派（复用 P3b 机制）
- **直接基元字段**：`FieldGetInstr(name)` 不变（编译器零改动）。运行时 name→slot(`field_index`)→composed
  offset → `decode_prim(bytes, off, kind)`。FieldIC 缓存 `TypeId→slot`（后取 composed offset/kind）。
- **直接引用字段**：同上解析到 composed offset → `ref_index(off)` → `refs[ri]`。写经 `write_barrier_field`。
- **内联 struct 叶子**：`StructFieldGetPrim/SetPrim(baked composed offset)` → 复用 `struct_field_get_val`
  的 `Value::Object` 臂（现读 `struct_bytes`/`struct_refs`+`inline_layout`，PR-2 改读 `bytes`/`refs`+composed 位图）。

### D11: static 存储字节化（修 REPL struct-in-static 悬垂）
`VmCore.static_fields: Vec<Value>` → 按 C# 静态存储块等价的「offset 字节内联」布局：static **struct**
字段内联字节进静态存储块（不再存帧作用域 `StructRef` 句柄），逃到 static 时**拷字节**；static **引用**
字段仍存句柄（一槽一引用，C# 亦如此）。根治 `ExprEmitter.z42` `StaticSet` 发裸 `StructRef` 逃逸悬垂。
带 e2e `struct_static_field` + REPL 回归验证。

### D15: static struct 字段用**装箱盒**实现（D11 落地方案，2026-08-14）

**实现选定 = 装箱盒（`BoxedStruct` 堆对象）而非 `static_fields` Vec 改字节块**：static struct 字段的槽
存一个 `Value::BoxedStruct`（堆 `ScriptObject`，进程存活 + GC 管 + **引用身份**）。比改 `VmCore.static_fields`
存储结构更小、复用已有 PR-2 装箱机制，且引用身份天然支持就地改。C# 值语义由**编译期 box/unbox 边界**给出：

- **整写** `Holder.P = v`（`ExprEmitter` 赋值臂 `_boxIfStaticStruct`）：`__box_struct(v)` 全局化——帧 arena
  blob 拷到堆盒（无悬垂），存盒句柄。是**值拷贝**（改 v/原值不影响 static）。
- **整读** `var q = Holder.P`（`Emit(BoundStaticGet)` struct 分支）：`AsCast` 拆盒回当前帧 arena `StructRef`
  独立副本（值语义，改 q 不影响 static）。
- **叶子读/写** `Holder.P.X` / `Holder.P.X = 5`：`_structChainRoot` 对 `BoundStaticGet(struct)` **不拆箱**、
  直接取盒句柄（`_emitStaticGetRaw`）；叶子 offset 由 `_structChainOffset` 从 struct 布局累积
  （`FieldByteOffset`，struct 相对）；runtime `struct_field_get_val`/`set_val` 新增 **`Value::BoxedStruct` 臂**
  （盒 bytes/refs 即 struct blob，用 **struct_layout**（非 composed object layout）解 offset）→ 就地读写盒（引用身份持久）。
- **判据**：`_isBlobStruct(field.Type())`（含 REPL `public static var v` 推断为 struct 的字段）。

**残留（deferred，小边角）**：`static Point P;` **未初始化**读（无 initializer → 槽 Null → 拆箱 Null →
`StructFieldGet(Null)` 崩）。well-formed 程序（含 REPL：`B b = new()` 先初始化）均先赋后读，不触发。彻底解 =
`DeclBinder` 为无 init 的 struct static 字段合成 `default(T)` 零 struct init（需构造 BoundExpr），列 follow-up。

### D12: 字段精确 tag 恢复 + per-field 访问表（2026-08-14 实施期发现，补 D10 缺口）

**发现的缺口**：D10 line 128 说「`decode_prim(bytes, off, kind)`」但没说 `kind`（精确 tag）从哪来。
PR-1 的 `ObjectLayoutDesc.field_kinds` 用的是**粗粒度 `StructLeafKind`**（`Prim=0`/`ArcString=1`/`GcRef=2`/
`Struct=3`），**无法**驱动 `decode_prim`——后者需精确 `ty::TAG_*`（同为 4 字节的 `i32`/`u32`/`f32` 解码
逻辑不同：符号扩展 vs 零扩展 vs 浮点）。`StructLeafKind.Prim + width` 二元组丢失了这个精度。

**解（不改格式，不 re-bump）**：精确 tag 从**字段声明类型串** `TypeDesc.fields[slot].type_tag` 恢复
——正是 `ObjNew` 的 `default_value_for(type_tag)` 已用的同一来源。运行时 `tag_from_name(type_tag)`
（`corelib/struct_reflect.rs`，`"int"→TAG_I32` / `"f64"→TAG_F64` / …）给出精确 `ty::TAG_*`，复用
`exec_struct::decode_prim`/`encode_prim`/`prim_width`/`is_ref_tag`（P3b 已有，struct 路同款）。

**per-field 访问表（避免每次访问 string-match，加载期预算一次）**：`ObjectLayout` 加运行时派生数组
`field_access: Box<[FieldAccess]>`（对齐 `fields`/slot 序），加载期从 composed offset + `fields[i].type_tag`
算出：
```
struct FieldAccess { offset: u32, width: u32, tag: u8, ref_slot: i32 }
// tag = tag_from_name(type_tag)（精确）；ref_slot = 若 is_ref_tag(tag) 则 composed.ref_index(offset)，否则 -1；
// struct-typed 直接字段：tag=TAG_UNKNOWN、ref_slot=-1（FieldGet 不发给 struct 根，只走 StructFieldGetPrim）
```
在 loader 组合出 composed `ObjectLayout` 后、`fields` 已知时 zip 填充（base 字段的 type_tag 在
`base.fields` 里，跨包 fixup 时随 `fields` 重建一并重算）。

**ScriptObject 访问 API（封装 decode/encode + refs 侧表，令 151 处 slots 迁移机械化）**：
```
impl ScriptObject {
  fn field_value(&self, slot: usize) -> Value          // prim: decode_prim(bytes) / ref: refs[ref_slot]
  fn set_field_value(&mut self, slot, v) -> WroteRef    // prim: encode_prim(bytes) / ref: refs[ref_slot]=v；返回是否 ref（调用方据此发 write_barrier）
}
```
FieldIC **格式不变**（仍缓存 `TypeId→slot`）；slot→offset/tag 走 `field_access[slot]`（一次数组索引，
非 string-match）。`StackObject`（stack_alloc arena 里的 `ScriptObject`）走**同一** API（同 bytes+refs 结构）。

### D13: 内联 struct 字段用 16B 布局（PR-1 的 8B 对象布局与 PR-2 的 16B struct 不兼容，2026-08-14 实施期发现）

**发现**：PR-1 的 `_computeObjFields` 用 **8B 引用版** struct 布局（`_objLayoutOfStruct`，「8B 终点」）
排布内联 struct 字段。但 PR-2 引用仍 16B（侧表），且 struct 各处（arena / boxed / `struct[]`）均用
**16B** 布局（`_compute`/`LayoutOf`）。二者对含引用的 struct **尺寸/嵌套偏移不同**（`Point{int;int;str}`：
8B 版 size 16、16B 版 size 24；`Line{Point;Point}` 的 `b.tag` 8B@24 vs 16B@32）。症状：反射
`FieldInfo.GetValue` 读对象内联 struct 字段时，`compute`（16B）算出的 nested ref offset 与对象 composed
ref 位图（8B）对不上 → `struct field reflection: ref leaf offset not in composed bitmap`；且裸叶子读
（`FieldByteOffset` 16B）与字段基址（`InlineFieldByteOffset` 8B）混用，对含 ref 的 struct 会错位。

**定解**：`_computeObjFields` 内联 struct 字段分支改用 **16B `LayoutOf`**（非 8B `_objLayoutOfStruct`），
使对象里的 struct 字段字节布局与独立 struct **逐字节一致**——反射读/装箱直接拷字节、嵌套 ref offset 一致、
裸叶子与基址同为 16B。直接引用**字段**仍 8B 洞（`_objSizeOf`，PR-3 填指针）；只有 struct **字段**转 16B。
**8B 终点留 PR-3 统一压缩**（对象布局 + struct 布局 + 侧表一起转 8B），不在 PR-2 拆散。

### D14: 内联 struct 字段判据与 offset 解耦 + tag 从 field_kinds 恢复（实施期两处 bug 修复）

- **判据 bug**：`ExprEmitter._isOwnerInlineField`/`_isInlineStructFieldRoot` 用
  `InlineFieldByteOffset(...) >= 0` 作「是否内联 struct 字段」判据。若让该函数对**所有**字段返 offset
  （错误地改用 `ObjectLayoutOf`），基元/引用字段被误判为内联 struct → 发 `StructAlloc` 而非 `FieldGet`
  → 读成 `<struct value>`/Null。**定解**：`InlineFieldByteOffset` **判据仍走 `InlineLayoutOf`**（只含
  struct 字段，非 struct 字段返 -1），确认是 struct 字段后 **offset 取 `ObjectLayoutOf`** 的 composed 值。
- **tag 恢复 bug（补 D12）**：`field_access` 的 tag 若纯靠 `tag_from_type_name(type_tag)`，用户类型别名
  （`using Id = int` → type_tag=`"Id"`）/FQ 名 leak 进 type_tag 时解析失败 → 误判 ref → decode 失败 → Null。
  **定解**：ref/prim/struct 分类**走编译器权威的 `field_kinds`（StructLeafKind）**（非 type_tag）；prim 的
  精确 tag 由 `resolve_prim_tag`（`tag_from_type_name` 识别则精确，否则按 `field_sizes` 宽度回落有符号整数
  tag）。**残留限制**：不透明的 float/char 别名回落成同宽整数 tag（罕见；彻底解=对象块带精确 tag，需格式 bump，deferred）。
- **反射 struct 字段类型名**：`object_inline_struct_field_get/set` 用 `struct_field_fq`（返 FQ 名）喂
  `compute`，非裸 `type_tag`（短名 `"Point"` 会 `unknown type`）。

### D16: PR-3 内部拆两 chunk + 标记指针机制 + 字符串引用收窄（2026-08-14 实施期定）

PR-3 的爆炸半径（引用全面 8B）实为两件正交的事，拆成两个 chunk 分别落地、各自可验：

**Chunk 1 —— `GcRef` 16→8B 标记指针（格式中立，本地 cargo+e2e 全验）**
- `GcRef` 从 `(NonNull<RegionEntry> 8B + generation:u32 4B + pad 4B)=16B` 压成**单个 8B 标记指针**：
  低 48 位是 RegionEntry 地址、高 16 位是**窄 generation 快照**（`refs.rs` `GCREF_ADDR_BITS=48`/
  `GCREF_ADDR_MASK`）。deref 一律经 `entry_addr()` 先 mask 掉 tag；`gen16()` 取高 16 位。
- 用 **strict-provenance** `ptr.map_addr()`/`.addr()`（Rust 1.84+，本仓 1.88）打/解 tag → 保留 entry
  的 provenance，**Miri-clean**（tag 只活在从不解引用的高位）。backing `RegionEntry.generation` 仍
  `AtomicU32`；只有句柄里的快照窄到 16 位（ABA 窗口 2^16，Decision 2 已接受的权衡）。
- 公共 API 全不变（`entry_ptr()` 返回 masked 地址、`ptr_eq` 比整个 tagged=地址+gen 一起比）。加
  `size_of::<GcRef>()==8` + `size_of::<Option<GcRef>>()==8`（NonNull niche 保住）静态断言防回退。
- **GcRef 是纯运行时表示、从不序列化 → chunk 1 零格式 bump**；`refs` 侧表仍存 `Value`（chunk 2 才内联）。
- 平台前提：x86-64 48 位规范 VA（用户态高 16 位=0）/ AArch64 48 位（Apple Silicon 47 位）→ 高 16 位可用。
  ARM MTE/PAC、5-level paging（LVA/57 位）会破 → task 3.3 CI 目标平台验证（本地 arm64 已过）。

**Chunk 2 —— 引用内联进 `bytes` + struct 16→8B + 格式 bump（PR-2 规模 big-bang，CI 收尾）**
- 编译器 `StructLayout._sizeOf` 引用 16→8（line 476），`LayoutOf`/`_compute` 全 8B → **D13 特判塌缩**
  （`_computeObjFields` 内联 struct 与 `_objSizeOf` 都 8B 了）→ 可删 `_objSizeOf`/`_objLayoutOfStruct` 死代码。
  struct 块 + 对象块（内联 struct 字段 offset）字节布局都变 → **格式 bump（zbc 1.34→1.35 / zpkg 0.39→0.40）**。
- runtime：object/array 引用把 8B tagged 指针**内联进 `bytes`**，`refs` 侧表**收窄到只存字符串**（见下）。
  `field_value`/`set_field_value` 按 tag 分派：prim→`decode_prim(bytes)`；object/array→读 8B 重建
  `Value::Object`/`Array`；str→侧表。GC `scan_object_refs`/`trace_children` 按对象级引用位图读 8B、
  按 kind 重建句柄 mark。JIT `jit_field_get/set` byte-offset。two-gen bootstrap（`_computeObjFields` 改
  编译器 → 自举字节动 → warm 重建 + gen1==gen2 校验）。

**字符串引用收窄决策（关键，避免双重格式 bump）**：`refs` 侧表存的是完整 `Value`，其中**字符串字段是
`Value::Str(Arc<str>)`=16B 胖指针**，而字符串 8B 细指针（`StrHeader`）是 **PR-4** 才做（Decision 3）。
故 chunk 2 **只把 object/array 引用（`GcRef`，已 8B）内联进 `bytes`，字符串引用仍留在收窄的 `refs`
侧表**（对齐 tasks「refs 侧表可删或**收窄**」的收窄分支）。PR-4 再把字符串转 8B 内联、彻底删侧表。
这样每个 PR 各一次干净 bump，不在 PR-3 先把字符串 16B 塞进 bytes、PR-4 又 8B 重排（双 bump 浪费）。
- **对象块 `ref_kinds` 需能区分 GcRef-object / GcRef-array / Str**：8B 裸指针本身不带「是 Object 还是
  Array」信息，重建正确的 `Value` 变体要靠 kind（现 `STRUCT_LEAF_GCREF` 粗粒度，object+array 都归它）→
  chunk 2 需把 kind 细分（这也是格式变更的一部分）。
- **Null 引用**：内联槽 8B=0 表示 `Value::Null`（`GcRef` 是 NonNull、不能表 null，故 null 走「0 指针」
  哨兵，`field_value` 见 0 返 `Value::Null`）。
- **`StackObject` 不可内联**（带 `frame_id`）：字段引用若指向栈分配对象即逃逸 → 应堆分配，故字段槽只会是
  堆 `Value::Object`/`Array` 或 Null；chunk 2 落地时加断言兜住（对称 static_set 现有的 StackObject 拦截）。
- **UB 敏感**（Decision 5 风险）：位图/offset/kind 任一错 → 扫错内存/重建错句柄 = UB。chunk 2 靠 golden +
  Miri/ASAN + 自举 5/5 逐字节三层兜底。

### D17: chunk 2 需**编译器权威 kind 细分**——粗粒度 GcRef 不足以安全内联（2026-08-14 深挖发现，关键）

深入 chunk 2 后发现「填 8B 洞」的图景**不安全**，根因是 **`StructLeafKind.GcRef` 把太多东西并一起**
（`_kindOf` 注释：「数组 / 非-struct class / interface / func / unknown → GcRef」），而这些在**运行时的
`Value` 表示各不相同**：

| 字段静态类型 | 运行时 `Value` | 能否内联为 8B GcRef |
|---|---|---|
| class / interface | `Value::Object(GcRef<ScriptObject>)` | ✅ 8B GcRef |
| array `T[]` | `Value::Array(GcRef<ArrayObj>)` | ✅ 8B GcRef（但重建变体≠object，须 kind 区分） |
| **delegate / func**（`Action`/`Func`/`delegate T F(..)`，z42 一等特性，`Delegates/` 整套子系统） | **`Value::Closure(Box)` / `Value::FuncRef(Box<str>)`** | ❌ **不是 GcRef**！内联读 bytes = 垃圾 = **UB** |
| string | `Value::Str(Arc<str>)` 16B | ❌ 侧表到 PR-4 |
| stack-escaped | `Value::StackObject`（带 frame_id） | ❌（逃逸到字段应已堆化，加断言） |

**结论**：安全内联必须在**字段级**区分「持 Object/Array（可内联 8B）」vs「持 Closure/FuncRef/Str（不可
内联）」。编译器现有粗粒度 `StructLeafKind.GcRef` **做不到**——delegate/func 字段和 class 字段都是 GcRef，
但运行时表示天差地别。→ **无「格式中立捷径」**：chunk 2 必须
1. **编译器细分 `_kindOf`**：把 delegate/func 类型（`IsDelegateType`/结构 func 类型）从 GcRef 里拆出来（新
   kind，如 `GcRefClosure`），class/interface→object-GcRef、array→array-GcRef、delegate/func→closure（侧表）。
   编译器有类型解析信息可判定（`_kindOf` 现只查 IsStructType/prim/string，需扩 delegate/func/array 判定）。
2. **对象块 `field_kinds`/`ref_kinds` 带细分 kind** → **格式 bump**（原以为格式中立的判断错了）。
3. runtime 按细分 kind 决定 inline（object/array）vs 侧表（closure/func/string）+ 按 object/array 重建变体。

**代价重估**：chunk 2 = 编译器 kind 细分 + 格式 bump + 全 runtime 迁移（all-or-nothing）+ GC + JIT +
Miri/ASAN + two-gen bootstrap CI。**比 opt-in 时以为的「填洞」大得多**，是 PR-2 规模、UB 敏感、需 CI 周期的
专项 big-bang。不能在无法本地完整验证的单会话里半提交。chunk 1（GcRef 8B）已作干净前置落地（293643f7）。

**下一步正确入口**（留待专门会话）：先做 **chunk 2a = 编译器 kind 细分 + 格式 bump，dormant**（对象块带细分
kind，runtime 暂不消费，additive+可 self-host 验、CI 出 fixtures，同 PR-1/task-2.0 范式）；再 **chunk 2b =
runtime 按细分 kind 内联 object/array + 收窄侧表**（消费 chunk 2a 元数据，可能格式中立、本地可验）。

#### 实现落点（chunk 2a 已落地，2026-08-14，zbc1.35/zpkg0.40）

最终实现比 D17 原计划的「改 `_kindOf`」更**收窄**——**不动 `_kindOf`**（它保持粗粒度 Prim/ArcString/GcRef/Struct），
只在**对象块直接字段**这一处细化，令 struct 布局 / 引用位图 / 尺寸对齐**全不受影响**（把格式变更面积压到最小）：

- **`StructLeafKind`**（`StructLayout.z42`）加 `GcRefArray=4` / `GcRefClosure=5` + `IsRef(kind)` helper（四类引用等宽同对齐，尺寸/位图判据统一走它）。
- 新 **`_refineDirectRefKind(ftype)`**：**只对 coarse==GcRef 的直接字段调用**，判据**保守**——
  ① 数组拼写 `T[]`（剥 `?` 后 `.EndsWith("[]")`）→ `GcRefArray`（运行时 `Value::Array`，可内联）；
  ② base 名（泛型剥 `<...>` + Canon）在 `_classDefs` 里 → `GcRef`（object/interface，`Value::Object`，可内联）；
  ③ 其余（**delegate/func**——不在 `_classDefs`，在 `SymbolTable.Delegates` 是 `Z42FuncType`；未解析泛型；unknown）→ `GcRefClosure`（侧表安全默认）。
  **false-negative（object 落 closure）只是次优（留侧表），绝不 UB**；反之误把 closure 判成可内联 = UB，故判据宁紧勿松。
- **`_computeObjFields`** 直接字段分支：`AddField` 用 `_refineDirectRefKind` 细化的 `fieldKind`，但 **`AddRefLeaf` 仍传粗粒度 `kind`**（引用位图 `ref_kinds` 不变）。→ **格式变更仅限对象块直接字段 `field_kinds` 字节**（array/delegate 字段 2→4/5）；`ObjectSize`/offset/size/`ref_kinds`/struct 块**逐字节不变**。
- **`_classDefs` 赋值提前**到 struct `LayoutOf` 循环**之前**（原在其后），使 object 字段在 struct 布局期也能被 `_refineDirectRefKind` 正确判成 GcRef（否则 `_classDefs==null` → 落 GcRefClosure）。
- **runtime `types.rs` 休眠**：`STRUCT_LEAF_GCREF_ARRAY=4`/`GCREF_CLOSURE=5` 常量 + `compose_object_layout` 把 4/5 **映射回粗粒度 GcRef 侧表路径**（`TAG_OBJECT` + `ref_slot`，与 PR-2 字节行为一致）→ 行为不变。**chunk 2b 翻转**：读 `field_kinds` 决定 object/array 内联 vs closure/string 侧表，并按 kind 重建 `Value` 变体。

**chunk 2b 为何可格式中立**：`field_kinds` 已是**权威的每直接字段 kind**；chunk 2b 在 `compose_object_layout`（runtime、load 期）据它重算「哪些直接引用内联进 bytes、哪些留侧表」+ GC 按 field 布局扫内联引用（object/array 变体由 field_kind 定），无需再改 wire → 不 bump。内联-struct 内部引用叶子（`ref_kinds` 粗粒度）在 chunk 2b **不触及**（struct 内联字节化是 2c），继续走侧表、粗粒度扫，行为不变。故 chunk 2a 只细化 `field_kinds`、不动 `ref_kinds` 是**正确且充分**的。

**本会话踩的坑**：`Edit replace_all` 把 `kind==ArcString||kind==GcRef` → `StructLeafKind.IsRef(kind)` 时**误伤 `IsRef` 方法体自身**（其体内正含该 pattern）→ `IsRef` 调自己 → 无限递归 → 栈溢出 SIGSEGV(139)，debug VM 无 panic/backtrace（栈溢出特征）。另：多次 build 后 `artifacts/build` 混版会假崩 139 → 隔离前必 `rm -rf artifacts/build/{compiler,libraries,toolchain}` 重来。
