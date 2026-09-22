# zpkg 包格式

> **页型**: 参考页 ｜ **状态**: ✅ 已实现（v0.43）｜ **代码**: `src/libraries/z42.ir/src/`（`ZpkgWriter.z42` / `ZpkgWriterIndexed.z42` / `ZpkgReader.z42`）
> **相关**: [zbc 字节码格式](zbc.md) · [工程模型、依赖解析与工作区编译](../../../internals/src/compiler/project-model.md) ｜ **对齐**: 2026-07-19

## 概述

`.zpkg` 是 z42c 把一个包的多个模块打成的分发单元：包级元数据 + 各模块的 zbc 内容。当前版本 **0.49**，与 zbc 1.44 强耦合（两者同步 bump）。

它有两种布局：**packed**（模块 zbc 字节内嵌，用于分发与测试）与 **indexed**（模块 zbc 外挂为散装 `.zbc` 文件，用于开发态增量）。字节原语与 section 目录结构与 [zbc](zbc.md) 一致，本页只列 zpkg 特有部分。

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

与 [zbc STRS](zbc.md) 逐字节同构（segment-dict 编码）。packed 模式池含全部模块的串；indexed 模式主文件池只含元数据 + SIGS + IMPL 串，散装 zbc 各自带局部池。

### NSPC — 命名空间表

`u32 ns_count` + `u32 × ns_count`（pool idx）。

### DEPS — 依赖表

`u32 dep_count`，每条 `{ file pool idx; u16 ns_count; u32 × ns_count }`——依赖 zpkg 文件名 + 它提供的命名空间，供 VM lazy 路由。

### SIGS — 全局签名表

`u32 total`（全模块函数总数）+ 平铺条目，每条与 [zbc SIGS](zbc.md) 逐字节同构。模块经 `first_sig_idx` 定位自己在此表中的首函数。

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
> [`docs/internals/src/formats/zpkg.md`](zpkg.md) within-minor 例外）。runtime
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

### 跨 zpkg `impl` 块传播 — IMPL section + Phase 3 merge（2026-04-26 cross-zpkg-impl-propagation）

#### 背景

L3-Impl1 让 `impl Trait for Type { ... }` 在同 CU 内工作（SymbolCollector 把
impl 方法合并进 target class 的 `Methods` 字典 + trait 加到
`_classInterfaces[target]`），但**跨 zpkg 不可见**：z42.numerics 给 z42.core
的 `int` 实现 `INumber<int>`，下游消费者读 z42.core TSIG 看不到这个 trait
→ `where T: INumber<int>` + `int` 类型实参编译报错。

#### IMPL section（zpkg v0.8）

zpkg 加新 section `IMPL`：每个 ExportedModule 携带本 CU 的 `impl Trait for Type`
列表（仅 declarations，方法 body 仍走 MODS section）。

```
IMPL section layout
─────────────────────
[Module Count: u16]
For each module:
    [Namespace pool idx: u32]   ← 与 TSIG 模块顺序一致（positional matching）
    [Impl Count: u16]
    For each impl:
        [Target FQ name pool idx: u32]   ← 例如 "Std.Int32"
        [Trait FQ name pool idx: u32]    ← 例如 "Std.INumber"
        [Trait TypeArg Count: u8]
        For each trait type arg: [Type string pool idx: u32]
        [Method Count: u16]
        For each method: WriteMethodDef(...)   ← 复用 TSIG 的 MethodDef 序列化
```

**重要**：IMPL 模块顺序与 TSIG 模块顺序一致（positional matching），不能按
namespace 索引 —— 一个包内多个 .z42 文件可能共享 namespace（z42.core 的所有
文件都用 `Std`），按 namespace 唯一索引会撞键。

### ImportedSymbolLoader Phase 3

`ImportedSymbolLoader.Load` 由两阶段扩展为三阶段：

```
Phase 1 — 骨架登记                ← 已有
Phase 2 — 成员填充                ← 已有
Phase 3 — impl merge (NEW)
  foreach module.Impls:
    targetClass = classes[short_name(impl.TargetFqName)]
    foreach method in impl.Methods:
      targetClass.Methods.TryAdd(method.Name, method.Sig)   // first-wins
    classInterfaces[short_name].Add(impl.TraitName)         // dedupe
```

冲突策略：first-wins（与 SymbolCollector.MergeImported `TryAdd` 一致）。
`target` FQ 名通过 `SplitFqName` 拆 `Std.Int32` → namespace `Std` + short `int`，
仅在 namespace 匹配 `classNs[short]` 时才合并（避免不同包同名类污染）。

### IrGen — QualifyClassName 对齐 imported target

`IrGen.cs` 早先用 `QualifyName(targetNt.Name)` 给 impl 方法注册 funcParams
和生成方法 body 的 IR 函数符号。当 target 是 imported（如 z42.numerics 给
z42.core `int` 加方法），这会把方法生成到错误命名空间（`numerics.int.op_Add`
而非 `Std.Int32.op_Add`），导致 VM `func_index` 注册符号与消费者 VCall 期望
不一致。修复：改用 `((IEmitterContext)this).QualifyClassName(...)`，imported
target 走 source namespace，local target 行为不变（等同 QualifyName）。

### VM 端零改动

方法 body 走 z42.numerics 自己的 MODS section，函数符号 `Std.Int32.op_Add`。
当用户代码 `using z42.numerics`，lazy loader 注册该 zpkg 所有函数到 `func_index`，
VCall(int_obj, "op_Add") 通过 `primitive_class_name(I64) = "Std.Int32"` + method
拼出 `Std.Int32.op_Add` → 命中 z42.numerics body。VM decoder 不需要解析 IMPL
section（基于 tag 查找天然跳过未识别 section）。

### 兼容性

zbc version 0.7 → 0.8。pre-1.0 规则：旧 zbc 不可读，需要 `./xtask build test`
重生。

---

## 泛型接口 dispatch — Z42InterfaceType.TypeParams（2026-04-26 fix-generic-interface-dispatch）

> 写出/读取实现：`z42.ir/src/ZpkgWriter.z42` 的 IMPL 段 · `ZpkgReader.z42` 按位置挂回 `Impls`。

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

### 两种构建形态

| 构建 | 主 `.zpkg` | sidecar | 栈跟踪 |
|---|---|---|---|
| debug（默认） | 内嵌 DBUG（LineTable + LocalVarTable） | 无 | `at <FQN>(<sig>) (<file>:<line>:<col>)` |
| release strip（`[profile.release].strip = true` 或 `--strip-symbols=true`） | 剥离 DBUG bodies + 写 16B BLID | `<name>.zsym`（`ZpkgFlags.SymOnly`，独立 STRS 子集 + MDBG + BLID） | 有 sidecar 时与 debug 无差异；无则 `at <FQN>(<sig>) +0x<offset>` |

### 加载期探测

loader 打开 `<path>/<name>.zpkg`（或 `.zbc`）后，按 stem 探同目录 `.zsym`：

```
load_zpkg(path):
  1. parse main zpkg (FUNC bodies, SIGS, ...)
  2. probe `<path-stem>.zsym`
     ├─ 存在 & SymOnly 位 & BLID 相等 → 按 index 把 MDBG 合入 per-module funcs
     ├─ 存在但 BLID 不符 / 损坏      → warn + 忽略（**加载不失败**）
     └─ 不存在                        → 静默退化（trace 走 fallback 形式）
  3. 照常走 load pipeline
```

**探测只看同目录**——没有 debuginfod 风格的环境变量 / URL 搜索路径。

### 帧签名来自 SIGS

trace 里每帧的函数名携带参数类型签名（`at MyApp.Greeter.greet(Greeter,str) (Greeter.z42:14:5)`）。
来源是 [SIGS](#sigs--全局签名表) 中每函数的 `paramCount × u32 strIdx`。实例方法把隐式 `this`
（类型 = 接收者类裸名）编为 index-0 条目；SIGS 里没有对应名称时（旧产物 / 合成函数）填 `?` 占位。

### Deferred

- **eager 加载**：sidecar 现在是一次性全量读，启动 IO 一次付清；启动延迟敏感场景可加 lazy / mmap 路径。
- **跨目录 sidecar 搜索**：见上，仅同目录。
- **stdlib 公开 `Std.Reflection.Symbolicate`**：让 z42 程序内部触发符号化，尚未提供。

## zpkg 与 zbc 的关系

- **packed**：每模块的 zbc FUNC/TYPE/DBUG/REGT/TIDX 段字节内嵌进 MODS，与独立 `.zbc` 逐字节同构，唯一区别是字符串池全局共享而非文件局部。SIGS 复用同一条目构建器。
- **indexed**：主文件只存目录 + 全局 SIGS，zbc 作散装文件外挂，主文件用 `zbc_hash` 校验一致性。

## 版本

Strict-pin，与 zbc 同政策；zpkg 版本与 zbc 版本强耦合（当前 0.49 ↔ 1.44），bump 联动。同步 checklist 见开发基础设施部分的 version-bumping 规范。

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

### 有**两个** reader，这条政策要各实现一遍（warn-on-zpkg-version-mismatch，2026-09-22）

`.zpkg` 有两个独立的读取实现，走的是完全不同的路径：

| reader | 谁在用 | 什么时候读 |
|---|---|---|
| `src/runtime/src/metadata/zbc_reader`（Rust） | z42vm | **运行期**加载包 |
| `src/libraries/z42.ir/src/ZpkgReader.z42`（z42） | z42c / z42b / REPL / 分析工具 | **编译期**跨包扫描（`DepScan.ScanDirs` 把 libsDirs 下所有 `z42.*.zpkg` 当数据盲读） |

上一节那套「点名 + 给补救命令」此前**只在 Rust 那边落地**；z42 侧的 `ZpkgReader.Open` 对版本
失配是一条光秃秃的 `return null`，一个字都不打。后果与上一节描述的一模一样，只是搬到了编译期：
依赖包被**整包跳过**，而「跳过」不会失败 —— 它在很远的地方以满屏

```
E0401: undefined: DiagnosticCodes
E0443: undefined type: Span
```

的形态浮出来。**真因是「整个包不见了」，症状却是「你引用了不存在的类型」**，中间没有任何桥。
实测为此二分过三轮。

现在 z42 侧也在**检测点**点名（`_warnVersionSkew`），三行：跳过了谁 + 它是哪个版本 / 为什么你
会在别处看到一堆 `undefined` / 怎么修。两点设计取舍：

- **按版本去重**：一个过期的 libs 目录常有几十个同代旧包，逐个报会把真信号淹在噪声里
  ⇒ 同一个 `<major>.<minor>` 只报一次，并明说同版本的其余包不再重复。
- **只有版本失配会出声**；坏 magic / 长度不足 / SymOnly sidecar 维持静默跳过 —— 那些确实
  可能是无关产物，与 Rust 侧「version mismatch 点名，其余 warn-and-continue」的分界一致。

`Open(byte[])` 保留原签名（委托给 `Open(byte[], string origin)`），`origin` 只用于告警措辞：
文件路径，或 REPL 那种内存包的包名。
