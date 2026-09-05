# zpkg 包格式

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（v0.43）｜ **代码**: `src/libraries/z42.ir/src/`（`ZpkgWriter.z42` / `ZpkgWriterIndexed.z42` / `ZpkgReader.z42`）
> **相关**: [zbc 字节码格式](zbc-format.md) · [工程模型、依赖解析与工作区编译](project-model.md) ｜ **对齐**: 2026-07-19

## 概述

`.zpkg` 是 z42c 把一个包的多个模块打成的分发单元：包级元数据 + 各模块的 zbc 内容。当前版本 **0.43**，与 zbc 1.38 强耦合（两者同步 bump）。

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

Section 目录同 zbc：每条 12 字节（tag 4B + offset u32 + size u32），首段偏移 `= 16 + sec_count × 12`。reader strict-pin，`major`/`minor` 任一不符即拒绝该文件（**不再静默跳过**，见 [版本](#版本)）。

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

`hash` 是**源码哈希**，形如 `mmh3:<32 hex>`（`Z42.Project.ZpkgBuilder.SourceHashHex`，MurmurHash3
x86_128）。它只服务增量构建的**变更检测**——「这个 `.z42` 与上次编译时是否一字不差」，纯相等性比较，
不参与信任决策；Rust 侧 `formats.rs` 只把它当不透明字段存取，从不重算或校验。理由与 [BLID](#blid--build-id)
同（解释执行下 SHA-256 需 6.38 G 指令 / 800 KB，Murmur3 只需 0.41 G）。算法前缀带在值里：`mmh3:` 与
2026-09-06 前的 `sha256:` 天然不等 ⇒ 跨版本混用的缓存一次全量失效，正是想要的语义。

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

16 字节 **MurmurHash3 x86_128**（`Z42.IR.Murmur3.Hash128`）。release strip 的主 packed 包先写 16 字节占位、装配后对全字节 hash 回填；sidecar 直接写同一 build id。runtime 据此把 sidecar 与主包配对。

> **为什么不是密码学哈希**：build_id 只做「这个 `.zsym` 配不配这个 `.zpkg`」的**配对识别**，不参与
> 任何信任决策；runtime 只**读取两个值比相等**，从不重算（`metadata::build_id::compute` 在整个
> runtime 里没有调用点）。而 z42c 是解释执行的，BLAKE3 在这条路径上贵得离谱——800 KB 输入实测
> BLAKE3-128 需 6.21 G 指令，MurmurHash3 x86_128 只需 0.41 G（**15×**）。选 x86 而非 x64 变体是因为
> z42 没有逻辑右移也没有无符号整数：x86 的 lane 是 32 位，用 `long` 承载 + `& 0xFFFFFFFF` 掩码即可
> 保证值恒非负、`>>` 等价逻辑右移，无需模拟 64 位无符号移位。（2026-09-06 前为 BLAKE3-128。）
>
> **注意**：indexed 包的散装 `.zbc` 内容哈希（`FILE` 段的 `zbc_hash`）**仍是 BLAKE3-128** ——
> 那个值 Rust 侧 `loader/artifact.rs` 会**重算校验**，是真的跨语言契约，与本节的 BLID 无关。

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

Strict-pin，与 zbc 同政策；zpkg 版本与 zbc 版本强耦合（0.43 ↔ 1.38），bump 联动。同步 checklist 见开发基础设施部分的 version-bumping 规范。

### 版本失配怎么表现（fix-version-mismatch-diagnosis，2026-09-05）

Strict-pin 是**双向**的：reader 只认与自己 writer 完全相同的 `major.minor`，比自己**旧**的
zpkg 同样读不了。所以 **一个 z42vm 与它加载的每一个 `.zpkg` 必须同代**，没有兼容层可退。

由此推出一条运行期规则：**版本失配不是「跳过这个文件继续跑」，而是致命的**。

| 失配位置 | 行为 |
|---|---|
| 入口 `.zpkg`/`.zbc` | 直接报错退出（一直如此） |
| `Z42_LIBS` 里的 **`z42.core.zpkg`** | **报错退出**，并给出补救命令 |
| 依赖 / 命名空间解析出的其它 `.zpkg` | 警告（一个命名空间可能有多个候选，未必致命），但警告文案里点明是版本失配 + 补救命令 |
| `.zsym` sidecar | 警告（调试符号是可选的，缺了只影响栈回溯可读性） |

改这条之前，`z42.core` 加载失败只是一条 `WARN`，程序照跑，直到很远处才以
`undefined function Std.IO.Environment.GetCommandLineArgs$0` 这种**完全误导**的形式炸掉
（比运行时更旧的 VM 上则直接挂死）。典型触发场景：仓库里 `install-z42.sh` 下载的
`.z42/` 种子还停在旧格式，而构建树已经跟着 main 的格式 bump 走到了新版本。

实现：`zbc_reader/versions.rs` 的 `FormatVersionMismatch`（带类型的错误，Display 文案与
历史字符串逐字相同）+ `app.rs` 从 anyhow 链里 `downcast` 出它来与「普通读失败」区分。
补救命令由错误类型自带：zpkg → `xtask build stdlib`，zbc → `xtask regen`；也可以改用
`Z42_PORTABLE_VM=<配套的 z42vm>` 反过来迁就产物。
