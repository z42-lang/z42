# tasks: type-section-flags2-and-struct-fields

> 类型：**ir + vm**（改 zbc/zpkg 格式）｜ 创建：2026-09-27
> 出身：审计裁决项 **D-4**（`class_flags2:u16`）+ 提案 `symbolic-struct-field-access` 的 **P0**。
> User 2026-09-27 批准 D-4，并裁「一起做」（两项同一次格式 bump）。

## Why

### ① `class_flags2: u16` —— 位用光的代价已经现形

`class_flags` 是 **u8 且 8 位已用满**（abstract / sealed / struct / record / interface / enum /
delegate / has-inline-struct）。后果不是「将来会不够」，而是**已经付过一次**：

zbc 1.33/1.34 的**对象全字段块**拿不到 flag 位，只能由一个**推导谓词**当闸门
（「非 struct/接口/枚举/委托」，写端写成 `(cd.Flags & 116) == 0`），而那个谓词在
**写端与 Rust 读端各抄一份** —— 正是结构审计 **R2「判据复制」** 的样本。

`class_flags2` 开的是位面：新块**按位 gated**，不必再抄谓词；也是「把分类判据下沉为数据位」
（审计对「六份基元名单 + 18 处镜像谓词」的收敛方向）的前置条件。

### ② 值 struct 的逐字段布局表 —— 单态化唯一活跃动因的解药

`StructFieldGetPrim/SetPrim` 的 `byte_offset` 今天由 codegen **烘焙成立即数**。而这正是
「泛型实例化必须有自己的体」的**唯一活跃动因**：`IrGen.InstNeedsOwnBody` 只有两条理由，
另一条 `DefHasStaticState` **实测在产品代码零命中** ——

| 测量（2026-09-27） | 数 |
|---|---|
| 全仓泛型类型声明 | **269** |
| 体内出现 `static` 的 | 5 |
| 真有静态**字段**的 | **2**（都在 `generic_static_per_instantiation.z42`，专测该特性的 fixture）|
| **产品代码**（编译器 / stdlib / 工具链 / examples） | **0** |

⇒ 承载逐字段偏移，是让运行期**有可能**自己解析的前置条件，即提案
`symbolic-struct-field-access` 的 **P0**。

⚠️ **数据零新计算**：`StructLayoutInfo.FieldOffsets / FieldSizes / FieldKinds` 编译器早就算好了，
此前只是没写进 wire（`ClassDescBuilder` 填 `StructSize`/`StructRefOffsets` 用的就是同一个对象）。

## What Changes（均 **A-support**：写、对称读、**无人使用**）

| 位置 | 改动 |
|---|---|
| TYPE 记录 | 紧随 `visibility` 加 **恒存在的 `class_flags2:u16`**（每记录 +2 字节，无 gate）|
| TYPE 记录 | `class_flags2` bit0 gated 的**逐字段布局表**：`field_count:u16 + (off:u32, size:u32, kind:u8)×n`，同序平行于 `fields` |
| 版本 | zbc **1.44 → 1.45**、zpkg **0.49 → 0.50** |

⚠️ 按 `bootstrap-seed.md`「**support 先行、晚一个 nightly 再 use**」：本 change 只承载，
消费留到下一刀。这与 zbc 1.34 对象块当年的做法一致（它的注释自称「PR-1 休眠元数据」）。

⚠️ 指纹**不追加 slug**：格式 minor 变化已让所有旧 `.meta` 失效（`version-bumping.md` 规则表第 2 行）。

## Scope

- `src/libraries/z42.package/src/IrModule.z42`（`Flags2` + `StructField*`）
- `src/libraries/z42.package/src/BinaryFormat/{ZbcWriter,ZbcReader,ZbcFormat}.z42`
- `src/libraries/z42.package/src/ZpkgWriter.z42`（minor）
- `src/compiler/z42c.semantics/src/ClassDescBuilder.z42`（填表 + 置位）
- `src/runtime/src/metadata/bytecode/class.rs`、`zbc_reader/{type_reader,versions}.rs`、`zbc_reader_tests.rs`
- `docs/internals/src/formats/{zbc,zpkg}.md`

## Tasks

- [x] `class_flags2:u16` 写端 / z42 读端 / Rust 读端三处对称
- [x] 逐字段布局表 三处对称（`CLASS_FLAGS2_HAS_STRUCT_FIELD_TABLE` 按位 gated）
- [x] `ClassDescBuilder` 填表（零新计算）+ 置 bit0
- [x] zbc 1.45 / zpkg 0.50，Rust 侧 `ZBC_VERSION_MINOR` / `ZPKG_VERSION_MINOR` + 版本 pin 测试同步
- [x] 格式文档两处（`zbc.md` 版本表新行、`zpkg.md` 两处版本号）
- [x] **Rust 侧 `cargo build --release` 通过**
- [x] **z42 侧 `z42.package` 用 0.49 种子编译器编过**（31 个文件、零诊断）——覆盖
      `ZbcWriter`/`ZbcReader`/`IrModule`/版本常量
- [ ] GREEN：CI 全矩阵绿（格式 bump 走 CI 的两代自举）

## 🔴 本地为什么没能跑完整套：`build compiler` 会自己重建 VM

格式 bump 的本地验证需要**两代自举**（旧 VM 跑旧 z42c 编当前源 → gen1；gen1 编 stdlib → 新格式）。
我备齐了零件（pre-bump 的 0.49 VM + 从 CI `toolchain-macos-26` artifact 拼出的 `.z42` 布局种子），
仍卡住，根因**实测确认**：

> **`xtask build compiler` 会把 VM 一起重建**（我在运行前后逐字节比对 VM 文件，确认「跑的过程中
> VM 被换了」）。于是在格式 bump 的分支上，它必然造出一个 **0.50 的 VM**，随后读不了 bump 前的
> 0.49 产物 —— gen1 这一步在本地自己把自己挡住了。

CI 的 `ci-bootstrap` 用**独立的两代 job**（SDK 自带的旧 VM，不重建）绕开这点，
`bootstrap-seed.md` 明写「格式 bump 的 build-and-test / toolchain-bootstrap / package 路径
**CI 自动过、免手动传种子**」，并且其中一段清理路径「**本地不可验**」。

⇒ 故本刀的 z42 侧验证做到「**改动的包能被旧代编译器编过**」为止，其余交 CI。
**未验证的残余 = `ClassDescBuilder.z42` 里 5 行赋值 + 1 行置位**（字段已逐个核对存在）。

## 途中的两条实测收获（都已写进相应文档/记忆）

1. **`xtask` 的代际校验会主动丢弃落后一代的 warm 树**并要求种子 SDK
   （`seed: in-tree warm is a format generation behind (…this tree writes 50) — discarding and cold-staging`）
   —— 这是 #773 那道门在真实格式 bump 上第一次对我生效，行为正确。
   ⚠️ 但它 cold-stage 时会用树内 `artifacts/.z42` 那个**过旧**的种子，把 `build-libs` 清空并塞进旧 stdlib
   ⇒ 之后一切报 `undefined function Std.IO.Environment.GetCommandLineArgs$0`。**树内种子太旧时要先换种，别让它 cold-stage。**
2. **拷 zpkg 供种必须用 `--release`**：开发态是 **indexed**（主文件 + 散装 `.zbc`），只拷主文件
   ⇒ 运行期 `undefined function …SourceHashHex$1$string`。这条记忆里有，我还是踩了一次。

## CI 反馈（第一轮）：两组测试红，都是我的改动的直接后果

`verify-selfhost` + 四条 `test-host` 报 `❌ z42c [Test]: 2 unit(s) failed (of 24)`：

| unit | 症状 | 根因 | 处置 |
|---|---|---|---|
| `Z42cIrConstraintBundleTests`（5 红）| `array index 51 out of bounds (len=51)` | **手写的 TYPE 记录字节 fixture** 里没有新加的 2 字节 `class_flags2` ⇒ 读端多吃 2 字节就越界 | ✅ 已在 `_typeSection` 的 `Visibility` 之后补 `WriteU16(0)`，并同步头注的布局说明 |
| `Z42cSemanticsZbcTests`（3 红）| golden hex 不等 | header 版本字段 | ✅ 三条 golden 的 `5a42430001002c00` → `…2d00`。**三条用例的源码都没有类声明** ⇒ TYPE 段只有 4 字节 `class_count=0`、新增的两项一个字节都不进来，故**仅 header minor 变**（沿 1.44 那次的同款判断） |

### 第二轮：字节基线（已在本地重生并入库）

`compile-test-assets` 两腿红在同一道门上：

```
committed zbc-format byte baselines are stale — regen rewrote them.
```

**🔴 我先前的判断错了，在此更正**：我说过「本地重生被 `xtask build compiler` 重建 VM 堵死」。
真正缺的**不是能跑的 VM**，而是**本分支自己产的 1.45 z42c + stdlib zpkg**。CI 的
`compile-test-assets` 走的是 `xtask-bootstrap-artifact`：*本 PR 自己* `toolchain-bootstrap`
的 artifact（已是 1.45 的 zpkg）＋ 一个本地 cargo 编的 VM，再跑 `build test`。

照这条配方本地复刻即通（下载本 PR 的 `toolchain-macos-26` → 铺进 `artifacts/` →
`cargo build --release` 出 1.45 VM → `build test`）：`Regenerated: 383 ok, 0 failed`。

重生结果**正好印证了格式改动的形状**，这比「门变绿了」更有说服力：

| fixture | 字节 | 解释 |
|---|---|---|
| `empty` | 231 → 231 | **无类声明** ⇒ TYPE 段只有 `class_count=0`，只有 header minor 那一字节变 |
| `cross-import-token` | 439 → 439 | 同上 |
| `strp-func-minimal` | 541 → **543** | 一个类记录 × 新增 `class_flags2` u16 = **+2** |
| `multi-method` | 999 → **1001** | +2 |
| `with-frcs` | 608 → **610** | +2 |
| `with-tidx` | 802 → **804** | +2 |

⚠️ `src/tests/zpkg-format/` 零漂移（`git status` 里没有它）—— zpkg 头的 minor 不进这些 fixture。

## 顺带发现：版本 bump 的 checklist 自己过期了

`src/runtime/src/metadata/zbc_reader/versions.rs:7-12` 的四条同步清单里**两条指向不存在的东西**：

1. 第 1 条 `src/compiler/z42.IR/BinaryFormat/ZbcWriter.cs` —— **C# 编译器 2026-06-26 已整体删除**；
2. 第 4 条 `src/tests/zbc-format/generate-fixtures.sh` —— **该脚本不存在**（真实命令是 `xtask regen`，
   写在 `src/tests/zbc-format/README.md` 里）。

**✅ 改了**（我先前写的是「本 change 不改它」—— 撤回那个决定）。理由：照这份 checklist 做
**必然漏掉基线重生**，而那一步正是本 PR 第二轮栽的地方。一份被本 PR 当场证伪的清单，
留着比改掉的 diff 代价更大。第 1 条改指真正的写端 `z42.package/src/BinaryFormat/ZbcWriter.z42`，
第 4 条改成 `xtask build test` 就地重写 + 必须 commit，并在注释里记下它腐坏过。

⚠️ 仍**不做**的是 `ZbcFormat.Minor` / `ZpkgWriter.Minor` 那两条 16KB 单行注释搬进独立
格式史文档 —— 那是同一个病（#897 的条目就是这么丢的），但属独立一刀。

## 不做（Out of Scope）

- **不消费这两项**（符号化访问的 P2/P3 是后续刀）。
- **不动 `class_flags`（u8）的既有位**，也不把对象块的推导谓词改成 `class_flags2` 位 ——
  那会改既有块的 gate、属行为变更，应独立一刀并单独验。
- **不把 `ZbcFormat.Minor` / `ZpkgWriter.Minor` 的 16KB 单行注释搬进文档**。它们患的是我刚给
  指纹治过的同一个病（两个 PR 同改一行 ⇒ 后合者整行覆盖、git 不报冲突 —— #897 的理由就是这么丢的），
  **值得单独一刀**；没顺手做是因为它会把本刀的 diff 搅大，且 User 未就此表态。
