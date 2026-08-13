# Proposal: 统一 struct/class 内存布局 + 引用压 8B（C# 等价，非移动 GC / 路 A）

## Why

z42 现在 struct 与 class 用**两套不同的字段存储模型**：

- **struct**（值类型）：`bytes` 字节区（基元 byte-pack）+ `refs` Value 侧表 + `StructTypeLayout` 引用位图 —— C 顺序布局，已 blittable。
- **class**（引用类型）：`slots: Box<[Value]>` —— 每字段一个 24B tagged `Value`，按 slot 下标访问，无字节布局。

后果：① class 的**基元字段**每个占 24B（`class{int x,y,z}` = 72B 槽，C# 12B）；② class 的**引用字段**每个占 24B（C# 8B 指针）；③ 引用本身 `GcRef` = 指针 8B + `generation` 4B + pad = 16B，`Value::Str` = `Arc<str>` 胖指针 16B，把 `Value` enum 钉在 24B；④ 内存不紧凑、cache 差、与 C 结构体布局不一致。

`add-struct-heap-inline`（P3b, zbc 1.32/zpkg 0.37）已经把「字节区 + 引用侧表 + 对象相对引用位图 + `StructFieldGetPrim/SetPrim` 对象基址访问 + zbc 元数据 + GC 扫描 + 写屏障 + JIT 桥接」整条范式**为内联 struct 字段这一子集打通**。本变更把这套范式**推广到 class 的全部直接字段**，并进一步把引用压到平台指针大小（8B），使 struct 与 class 收敛到**同一套 C# 式 byte-offset 布局**。

## What Changes

**终点 = C# 完全等价**（User 2026-08-12 裁决「选项 3 / 一把梭到 8B」）：

1. **对象直接字段统一到 byte-offset 布局**：class 的所有直接字段（基元 + 引用 + 内联 struct）进对象相对字节布局；删除 `slots: Box<[Value]>`。基元 byte-pack（自然宽度），引用**内联为 8B 裸指针**，内联 struct 扁平嵌入。
2. **引用压 8B（路 A：标记指针）**：`GcRef` 16B→8B —— 把窄 `generation` 塞进 48 位虚拟地址高 16 位（tagged pointer），deref 时 mask；**保留现有 region + 非移动 GC**（不引入移动/分代 GC，那是后续 §6/P3）。
3. **字符串压 8B**：`Arc<str>`（ptr8+len8=16B）→ 细指针 `Arc<StrHeader{len,[u8]}>` = 8B，长度进堆对象头。
4. **`Value` enum 24B→16B**：最大 payload 降到 8B → `Value` = tag(8) + payload(8) = 16B。寄存器 / 数组 boxed 元素 / 侧表全部 33% 密度收益。
5. **GC 精确扫描切位图**：对象引用扫描从「逐 slot 看 Value tag」改为「按对象级引用位图读内联 8B 指针」。
6. **JIT 硬编码更新**：`STRIDE=24 / PAYLOAD=8` 的 slot 原生寻址 → 16B Value 寻址 + byte-offset 字段访问。
7. **格式 bump**：zbc/zpkg minor bump（对象完整字节布局表 + 引用位图入 TYPE section）。

**交付纪律（事实校正，User 已认）**：终点锁死 C（不中途停在 16B 引用），但**实现拆成内部可全绿的阶段 / 多个 PR**，每 PR 独立 GREEN、可 rebase —— 不做「单个巨红不可回退 PR」（违反 workflow 阶段 8）。tasks.md 按阶段编排。

## Scope（允许改动的文件）

> 本表为**程序总 Scope**；具体某个 PR 只触及其子集，PR 描述里再收窄。

### 运行时（Rust VM）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/types.rs` | MODIFY | `ScriptObject` 删 `slots`、扩 `struct_bytes`/`struct_refs` 覆盖全字段；`StructTypeLayout` 扩为对象完整布局；`Value` enum payload 收窄；`trace_children` 改位图扫；`default_value_for`/`inline_region_sizes` |
| `src/runtime/src/gc/refs.rs` | MODIFY | `GcRef` 16B→8B：标记指针（窄 generation 进高位）+ mask deref |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `alloc_object` 尺寸；`scan_object_refs`/mark/sweep 改位图；`write_barrier_field` 语义（byte-offset）；字节大小统计 |
| `src/runtime/src/gc/heap.rs` | MODIFY | 写屏障 trait 签名（若需） |
| `src/runtime/src/metadata/string_rep.rs`（NEW 或就近） | NEW | `StrHeader{len,[u8]}` 细指针字符串表示；替换 `Arc<str>` 用点 |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `FieldGet/FieldSet` 直接字段改 byte-offset 访问；`FieldIC` 缓存 byte-offset+kind |
| `src/runtime/src/interp/exec_struct.rs` | MODIFY | 对象基址访问推广到全字段；引用叶子内联 8B |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 读对象完整布局表；zbc/zpkg minor 常量 + changelog |
| `src/runtime/src/metadata/loader.rs` | MODIFY | 装 `TypeDescCold` 的对象布局 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | CLASS_FLAG / 布局描述结构 |
| `src/runtime/src/jit/translate.rs` | MODIFY | Value STRIDE 24→16；字段 byte-offset 原生访问 |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | slot helper → byte-offset |
| `src/runtime/src/jit/helpers/value.rs` | MODIFY | Value 布局注释/断言 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `FieldInfo.Get/SetValue` 直接字段改 byte-offset |
| `src/runtime/src/corelib/*.rs`（string/convert 等） | MODIFY | 字符串表示迁移触达点 |
| `src/runtime/src/host/marshal.rs` / `native/marshal.rs` | MODIFY | String tag payload 随细指针调整 |
| `src/runtime/crates/z42-abi/src/lib.rs` | MODIFY | 若 Z42Value payload 语义随之更新（评估） |
| `src/runtime/crates/z42-abi/tests/abi_layout_tests.rs` | MODIFY | ABI 布局断言更新 |

### 编译器（z42c）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/StructLayout.z42` | MODIFY | 从「仅 IsBlobStruct 字段」扩为「对象全字段完整布局」（基元+引用+内联 struct 统一 offset + 引用位图带 kind） |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 发对象完整布局表；不再为 struct 字段留死 slot |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `obj.x` 直接字段 codegen 改 byte-offset 指令；两条路合一 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | zbc `Minor++` + changelog |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | 写对象完整布局表 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | z42 侧 reader 镜像 |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | zpkg `Minor++` + 内嵌 zbc 版本 |
| `src/compiler/z42c.pipeline/src/CacheStore.z42` | MODIFY | `CompilerFingerprint++`（codegen 变） |

### 文档 / 测试

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/design/runtime/object-abi.md` | MODIFY | §3「slots=Value[]」修订为统一 byte-offset；§2.1 从 Deferred 提为已采纳（路 A）；§5 字符串细指针 |
| `docs/design/runtime/zbc.md` / `zpkg.md` | MODIFY | Minor changelog |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 「堆内联」段扩为全字段统一布局 |
| `docs/roadmap.md` | MODIFY | 385/386 状态更新 |
| `src/runtime/src/metadata/types.rs`（value_layout 单测） | MODIFY | Value 16B 断言 |
| `src/tests/zbc-format/*/source.zbc` | MODIFY | 格式 bump fixture 重生 |
| `src/tests/zpkg-format/*/source.zpkg` | MODIFY | 同上 |
| `src/tests/e2e/...`（NEW golden） | NEW | 对象字段布局 / 引用 8B / 字符串 / GC / 跨包 / 反射 端到端 |
| `src/compiler/z42c.semantics/tests/layout/*` | MODIFY | 对象完整布局单测 |

## Out of Scope

- **移动 / 分代 GC**（object-abi §6：forwarding / card table / pin 区 / young-old）—— 本变更保持**非移动 region GC**（路 A）；移动 GC 是独立后续程序。
- **NaN-boxing**（把 tag 也塞进 payload 做到 8B Value）—— 本变更 Value = 16B（tag+8B payload），不做 NaN-box。
- **C 互操作 marshal 接线**（对象/struct 直传 C 的 FFI 路径）—— 用户明确「先不考虑 interop」；布局统一后另起变更。
- **AOT 后端**（interp 全绿前不碰；JIT 在本变更内更新，AOT 顺延既有纪律）。

## Open Questions

- [ ] 窄 generation 位宽（塞进指针高 16 位）够不够抗 ABA？需评估当前 generation 回绕窗口 vs 16 位。若不够，路 A 需降级或引入辅助校验。
- [ ] `Value::Str` 细指针后，`Length` 热路径多一次 deref 的实测代价（string-heavy 的 z42c 自编译需 benchmark）。
- [ ] 格式 bump 与 bootstrap 两代自举的时序（version-bumping.md）—— 是否踩到 nightly 种子窗口。
- [ ] 交付切分：几个 PR？建议边界（① 编译器发全字段布局表 + reader，行为不变 → ② runtime 切 byte 存储 + GC 位图 → ③ 引用 8B 标记指针 → ④ 字符串细指针 → ⑤ Value 16B + JIT）。待 design 定。
