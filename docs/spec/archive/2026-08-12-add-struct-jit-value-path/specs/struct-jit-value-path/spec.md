# Spec: JIT struct 值路径

## ADDED Requirements

### Requirement: JIT 编译含 struct 值类型指令的函数

含 `StructAlloc` / `StructCopy` / `StructFieldGetPrim` / `StructFieldSetPrim` 任一指令的函数
**不再整体回退 interp**，而是被 JIT 编译，struct 指令经 helper call 执行于 `ctx.struct_arena`。

#### Scenario: 本地 struct 值语义（分配 + 字段读写 + 复制独立性）
- **WHEN** JIT 编译一个函数，函数内 `Point p; p.x = 3; Point q = p; q.x = 99;`（多字段 blob struct）
- **THEN** `jit_struct_alloc` 在 arena 分配零初始化 blob，寄存器持 `StructRef{idx, frame_id}`（frame_id
  来自该 JIT 帧）；`jit_struct_field_set_prim`/`get_prim` 按烘焙 byte_off 读写基元叶子；`jit_struct_copy`
  逐字节复制 blob → `q.x=99` 不影响 `p.x`（值语义）；执行结果**逐值等价 interp**

#### Scenario: 嵌套 struct 字段（累积 offset）
- **WHEN** JIT 函数访问 `line.a.x`（`struct Line{ Point a; Point b; }`）
- **THEN** codegen 烘焙的复合 byte_off 经 `jit_struct_field_get_prim`（base=arena StructRef）读到正确
  叶子，等价 interp

#### Scenario: 引用叶子（string/object 字段）
- **WHEN** JIT 函数读写 struct 的 string 引用叶子（如 `b.tag = "hi"`）
- **THEN** helper 走 arena `StructSlot::refs` 侧表（`Arc::clone` 语义），非裸字节；GC 扫描 arena 根不受影响

#### Scenario: 帧退出后 struct 局部释放（LIFO + 悬垂校验）
- **WHEN** JIT 函数返回，其 struct 局部所在 arena 槽被 `pop_frame` LIFO 截断
- **THEN** 后续帧复用该 arena 区间；因每个 JIT 帧持唯一 `frame_id`，任何陈旧 StructRef 解引用时
  frame_id 不符 → 抓 use-after-free（`bail!`），而非静默错读

### Requirement: JIT 下 struct[] 元素访问

`arr[i].x`（`Point[]`，StructBytes backing）在 JIT 下产 `StructRefHeap` 句柄，字段访问命中数组字节
backing——而非退化为 BoxedStruct 快照。

#### Scenario: struct[] 元素字段读写
- **WHEN** JIT 函数执行 `arr[i].x = 5`（`arr: Point[]`）
- **THEN** `jit_array_get` 对 `ArrayBacking::StructBytes` 产 `Value::StructRefHeap{arr, index}`（非
  `get_boxed`→BoxedStruct）；`jit_struct_field_set_prim`（base=StructRefHeap）写进数组字节 backing 的
  第 i 个元素叶子，就地生效（等价 interp #170 路径）

#### Scenario: foreach over struct[]（值副本循环变量）
- **WHEN** JIT 函数 `foreach (Point p in arr) { ... }`
- **THEN** 元素经 `AsCast`（StructRefHeap 臂拷出到帧 arena StructRef，需 frame_id）产值副本循环变量，
  等价 interp #173；循环体改 `p.x` 不影响数组元素

### Requirement: JIT 下 struct 装箱/拆箱

#### Scenario: 拆箱 `(Point)o`（BoxedStruct → arena StructRef）
- **WHEN** JIT 函数执行 `Point p = (Point)o`，`o` 是装箱 struct
- **THEN** `jit_as_cast` 对 `BoxedStruct` 精确类型匹配时**拆箱**——把堆 blob 拷回当前 JIT 帧 arena，返回
  `StructRef{idx, frame_id}`（依赖新增 frame_id）；类型不匹配返 Null。**不再保持 boxed 回退 interp**

#### Scenario: 装箱 `object o = p`（struct → BoxedStruct）
- **WHEN** JIT 函数把值 struct 擦除到 object（走 `__box_struct` builtin）
- **THEN** 已有路径不变（BoxedStruct 走堆、无需 frame_id）——本变更不改装箱，仅补齐拆箱

## MODIFIED Requirements

### Requirement: JIT 遇 struct 值类型指令的行为

**Before:** JIT 遇 `StructAlloc`/`StructCopy`/`StructFieldGetPrim`/`StructFieldSetPrim` 任一 →
`bail!("JIT cannot translate struct value-type instructions yet")` → 整函数回退 interp；
`jit_as_cast` 对 BoxedStruct 精确匹配保持 boxed（拆箱 interp only）；`jit_array_get` 对 struct[]
用 `get_boxed`→BoxedStruct 快照。

**After:** JIT 把 4 条 struct 指令 emit 为 helper call（操作 per-context struct_arena）；`jit_as_cast`
拆箱 BoxedStruct→arena StructRef；`jit_array_get` 对 StructBytes 产 StructRefHeap。函数不再因 struct
指令 bail。struct 指令本身经 helper 执行（≈interp 速度），周边算术/控制流/调用为 native（整函数收益）。

## IR Mapping

无新 IR 指令、无 zbc/zpkg opcode 变更。复用现有 `StructAlloc`(0xC0)/`StructCopy`/`StructFieldGetPrim`
(0xC2)/`StructFieldSetPrim`(0xC3) + `ArrayGet` + `AsCast`。纯 JIT 后端翻译新增，**格式中立**。

## Pipeline Steps

受影响阶段（仅 VM 后端）：
- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及
- [ ] TypeChecker — 不涉及
- [ ] IR Codegen — 不涉及（z42c 零改动，self-host 逐字节不变）
- [ ] VM interp — 不涉及（interp 已完备；仅把其字节编解码自由函数可见性提为 pub(crate) 供复用）
- [x] **VM JIT** — 4 struct helper + frame_id + array_get/as_cast 补齐
