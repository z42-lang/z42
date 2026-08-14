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
