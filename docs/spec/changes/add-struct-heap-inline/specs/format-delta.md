# Format delta: zbc 1.31→1.32 / zpkg 0.36→0.37（P3b 内联字段表）

> 以 **D1-a（基元裸内联 + 引用侧表）+ 路线 α（复用 StructRef 地址句柄，不加 opcode）** 为基线。
> D1 若改选 D1-b/β，本文对应节重写。精确坐标见 version-bumping.md 表。

## 复用（无新增 wire）

- **struct 自身 ref 位图**（zbc1.31 已持久化，`ZbcWriter.z42:346` / `zbc_reader.rs:568`）：
  `u32 size + u16 ref_count + (u32 off, u8 kind)×n`，gate=`CLASS_FLAG_STRUCT(0x04)`，kind 1=ArcString/2=GcRef。
  P3b 的内联字段直接查内联 struct 类型的这份位图 → **对象级 ref-offset 表由运行时组合导出**，不重复入 wire。
- **StructFieldGetPrim/SetPrim/StructAlloc/StructCopy**（opcode 0xC0–0xC3）：base 语义从「仅 arena StructRef」扩到「arena 或堆对象地址句柄」，**opcode 与编码不变**（Value::StructRef 变体扩，纯运行时）。

## 新增 wire：TYPE section 每个 class 记录的「内联字段表」

现有 class 记录含字段列表（name + type_tag 等）。P3b 为**每个字段**补 2 项，并为 class 补 1 项：

```
// 每字段（在现有字段编码后追加）：
inline_kind: u8        // 0 = 普通字段（占 slots，照旧）；1 = 内联 struct 字段（占 struct_bytes）
// 若 inline_kind == 1：
  inline_byte_off: u32 // 该 struct 字段在对象 struct_bytes 内的起始偏移

// 每 class（记录尾部，仅当存在 ≥1 内联字段时；由 flag 位 gate）：
inline_bytes_total: u32   // struct_bytes 总大小（也可由内联字段末尾推导——D1 后定是否省略）
inline_refs_total:  u16   // struct_refs 总格数（同上，可推导）
```

- **gate**：新增 `CLASS_FLAG_HAS_INLINE_STRUCT`（拟 `1<<3 = 0x08`，`bytecode.rs:124` 邻位）。类无内联字段 → flag 不置 → 尾部块不写 → **现有类逐字节不变**（自举/golden 零回归的格式保证）。
- **推导优化**：`inline_bytes_total`/`inline_refs_total` 可由「所有内联字段的 (off + 其 struct size / ref_count)」运行时推导 → 若采用则不入 wire，最小化格式面。IMPL 时二选一（倾向推导，wire 只加 per-field 的 `inline_kind` + `inline_byte_off`）。

## Writer 侧（z42c）

- `IrClassDesc`（`IrModule.z42:72`）加 `InlineFieldKinds:int[]` / `InlineFieldOffsets:int[]`（+ 若不推导则 `InlineBytesTotal`/`InlineRefsTotal`）。
- `ClassDescBuilder.z42`（`:221` 设 flag、`:228` 填 struct 位图那段）：为**含内联 struct 字段的 class**（字段类型 `IsBlobStruct`）算每字段 inline_kind/offset，置 `CLASS_FLAG_HAS_INLINE_STRUCT`。
- `ZbcWriter.BuildType`（`:346` 邻近）：flag 置位时写内联字段表。

## Reader 侧（Rust）

- `zbc_reader.rs`（`:568` 邻近 read_type）：读 `CLASS_FLAG_HAS_INLINE_STRUCT` → 解析每字段 inline_kind/offset → 填 `TypeDesc`（新增字段布局：区分 slot 字段 vs 内联字段 + 内联 byte offset + 内联 struct type name）。
- `bytecode.rs`：`StructLayoutDesc` 旁加 class 级内联布局描述（或扩 `TypeDescCold`）。

## Fixture / golden（version-bumping 步 4/5/9）

- `xtask build test` 自动重生 zbc-format 6 fixture（`empty`/`strp-func-minimal`/`multi-method`/`with-tidx`/`cross-import-token`/`with-frcs`）——它们无内联字段 → 仅 header minor 字段变（31→32），块内容不变。
- z42c golden hex 单测（`zbc_tests.z42` `test_zbc_empty_byte_identical`）重截 `empty/source.zbc` hex（`xxd -p`）。
- zpkg-format 4 fixture 手工 `z42c build` 重生（recipe 见 [[struct-value-semantics-program]]：packed=`build <toml> --release`；indexed=`build <toml,pack=false>` 拷 source.zpkg+source.zbc；sym-only 无字节读测试、陈旧不阻塞）。
- changelog：`docs/design/runtime/zbc.md` + `zpkg.md` 各加一行（minor / 日期 / spec=本 change / 引入=内联字段表）。
