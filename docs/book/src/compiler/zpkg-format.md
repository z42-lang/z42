# zpkg 包格式

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（v0.32）｜ **代码**: `src/libraries/z42.ir/src/`（`ZpkgWriter.z42` / `ZpkgWriterIndexed.z42` / `ZpkgReader.z42`）
> **相关**: [zbc 字节码格式](zbc-format.md) · [工程模型、依赖解析与工作区编译](project-model.md) ｜ **对齐**: 2026-07-19

## 概述

`.zpkg` 是 z42c 把一个包的多个模块打成的分发单元：包级元数据 + 各模块的 zbc 内容。当前版本 **0.32**，与 zbc 1.27 强耦合（两者同步 bump）。

它有两种布局：**packed**（模块 zbc 字节内嵌，用于分发与测试）与 **indexed**（模块 zbc 外挂为散装 `.zbc` 文件，用于开发态增量）。字节原语与 section 目录结构与 [zbc](zbc-format.md) 一致，本页只列 zpkg 特有部分。

## 文件布局

### 文件头（16 字节）

```
偏移  字段        宽度   值
0     magic       3 B    ASCII "ZPK"
3     (补零)      u8     0x00
4     major       u16    0
6     minor       u16    32
8     flags       u16    见下
10    sec_count   u16
12    reserved    u32    0
```

**flags**：`bit0 (0x01)` Packed、`bit1 (0x02)` Exe、`bit2 (0x04)` SymOnly（`.zsym` sidecar，reader 见此位即拒绝作工程包加载）。

Section 目录同 zbc：每条 12 字节（tag 4B + offset u32 + size u32），首段偏移 `= 16 + sec_count × 12`。reader strict-pin，`major`/`minor` 任一不符即静默跳过。

### Section 顺序

| 模式 | 固定段 | 可选 |
|------|--------|------|
| packed | `META` `STRS` `NSPC` `DEPS` `SIGS` `MODS` | `IMPL`（有导出成员时）、`BLID`（release strip 的主包末尾） |
| indexed | `META` `STRS` `NSPC` `DEPS` `SIGS` `FILE` | `IMPL` |
| sidecar (.zsym) | `META` `STRS` `MDBG` `BLID` | — |

packed 与 indexed 只在"模块体段"不同（`MODS` ↔ `FILE`），其余段共用同一构建器。

## Sections

### META — 包元数据

`str name` + `str version` + `str entry`（lib 的 entry 为空串）。

### STRS — 字符串池

与 [zbc STRS](zbc-format.md) 逐字节同构（segment-dict 编码）。packed 模式池含全部模块的串；indexed 模式主文件池只含元数据 + SIGS + IMPL 串，散装 zbc 各自带局部池。

### NSPC — 命名空间表

`u32 ns_count` + `u32 × ns_count`（pool idx）。

### DEPS — 依赖表

`u32 dep_count`，每条 `{ file pool idx; u16 ns_count; u32 × ns_count }`——依赖 zpkg 文件名 + 它提供的命名空间，供 VM lazy 路由。

### SIGS — 全局签名表

`u32 total`（全模块函数总数）+ 平铺条目，每条与 [zbc SIGS](zbc-format.md) 逐字节同构。模块经 `first_sig_idx` 定位自己在此表中的首函数。

### MODS — 模块体（仅 packed）

`u32 module_count`，每模块：

```
ns             pool idx
src            pool idx（源文件）
hash           pool idx（源码哈希）
fn_count       u16
first_sig_idx  u32（本模块首函数在全局 SIGS 中的下标）
func_len  u32 + func 体      （zbc FUNC 段字节）
type_len  u32 + type 体      （zbc TYPE 段字节；无则 0）
dbug_len  u32 + dbug 体      （release strip 时 0）
regt_len  u32 + regt 体
tidx_len  u32 + tidx 体      （无测试则 0）
```

五个 `len + 体` 即各模块内嵌的 zbc section 字节，用同一套 `ZbcWriter` 构建器产出，但共享 zpkg 全局字符串池（加 per-module remap 与 token 分配）。

### FILE — 模块目录（仅 indexed）

`u32 module_count`，每条：

```
ns             pool idx
src            pool idx（项目相对源路径）
src_hash       pool idx
fn_count       u16
first_sig_idx  u32
zbc_hash       pool idx（散装 zbc 内容 SHA-256，一致性校验）
```

头五字段与 MODS 对齐（故与 SIGS 配对方式相同），但**不内嵌 zbc 体**——zbc 作为散装文件外挂，装载时按 `src` 定位 `<pkgDir>/<src 去 .z42>.zbc`。

### IMPL — 跨包 impl 块

`u16 exported_count`，每模块 `{ ns pool idx; u16 impl_count; impl × }`。每 impl：`target_fq` + `trait_fq` + `u8 type_arg_count` + `u32 × type_arg` + `u16 method_count` + 方法定义。方法定义含名、返回类型、可见性、`u8 flags`（bit0 static / bit1 virtual / bit2 abstract）、min_arg、param_count、params_from、各参数（名 + 类型名）。

### MDBG — 调试信息（仅 sidecar）

`u32 module_count`，每模块 `{ ns_idx; u32 funcCount; frameName_idx × funcCount; u32 dbug_len; dbug 体 }`
（dbug 体同 zbc DBUG，用符号池）。`frameName_idx[i]` 与 dbug 的第 `i` 个函数行表按 index 对齐，
存该函数的 **frame-name(带签名) key**（`ns.Name(t0,t1)`，镜像 runtime `format_frame_name`）——使
`.zsym` **自足**映射「函数名 → 行表」，供离线 `z42d symbolicate` 还原剥离档崩溃栈的
`at <name>(<types>) +0x<offset>`（offset 打包 `(block<<16)|instr`）为 `file:line:col`。

> **frameName 是 within-minor 33 演进**（add-offline-symbolication, 2026-08-04）：MDBG 只在临时、
> 每次 release 重生的 `.zsym`（非分发稳定件），写读同版落地，故**不 bump 共享 minor**（依据见
> [`docs/design/runtime/zpkg.md`](../../../design/runtime/zpkg.md) within-minor 例外）。runtime
> 加载相邻 .zsym 时按 index merge（跳过 frameName，名来自主包）；z42d 离线时用 frameName 直接查。

### BLID — Build ID

16 字节 BLAKE3-128。release strip 的主 packed 包先写 16 字节占位、装配后对全字节 hash 回填；sidecar 直接写同一 build id。runtime 据此把 sidecar 与主包配对。

## Packed vs Indexed

| 维度 | Packed | Indexed |
|------|--------|---------|
| flags bit0 | 置位 | 不置 |
| 模块体 | MODS（内嵌 func/type/dbug/regt/tidx） | FILE（仅目录 + zbc_hash） |
| zbc 位置 | 内嵌主文件 | 散装 `.zbc` 外挂 |
| 字符串池 | 主文件全量 | 主文件仅元数据/SIGS/IMPL 串 |
| strip / sidecar | 支持 | 无（开发态 debug-only） |
| 用途 | 分发、单包、测试 | 开发态增量（未变文件 zbc 字节不动） |

## sidecar（.zsym）

release strip 时，调试信息剥离到旁挂 `.zsym`：flags = `Packed | SymOnly = 0x05`，段集 `META / STRS(符号串) / MDBG / BLID`。reader 遇 SymOnly 位拒绝作工程包，仅由专门入口按 build id 与主包配对后把调试信息合入。

**两种消费路径**（add-offline-symbolication）：① **运行时自动合并**——loader 探测与主包同目录的
`.zsym`，build_id 匹配则按 index 把行表 merge 回模块 → 栈跟踪直接出 `file:line:col`（`.zsym`
不在旁 → 栈出 `at <fn> +0x<offset>`）。② **离线符号化**——部署常不带 `.zsym`；归档 `.zsym` 后用
`z42d symbolicate <trace> --syms <file|dir>...`（多路径递归，参考 addr2line/Breakpad）据 MDBG 的
frameName → 行表 把 `+0x<offset>` 还原成 `file:line:col`。z42 侧读 `.zsym` 见 `z42.ir` 的 `SidecarReader`。

## zpkg 与 zbc 的关系

- **packed**：每模块的 zbc FUNC/TYPE/DBUG/REGT/TIDX 段字节内嵌进 MODS，与独立 `.zbc` 逐字节同构，唯一区别是字符串池全局共享而非文件局部。SIGS 复用同一条目构建器。
- **indexed**：主文件只存目录 + 全局 SIGS，zbc 作散装文件外挂，主文件用 `zbc_hash` 校验一致性。

## 版本

Strict-pin，与 zbc 同政策；zpkg 版本与 zbc 版本强耦合（0.32 ↔ 1.27），bump 联动。同步 checklist 见开发基础设施部分的 version-bumping 规范。
