# 嵌入式运行与测试 agent（app-run 核心 · bundle · 栈预算）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：
> `src/runtime/src/app.rs`（`z42::app::run` 核心）、`src/runtime/src/host/mod.rs`（C ABI
> `z42_host_run_app`）、`src/runtime/include/z42_host.h`（公有头）、
> `src/runtime/crates/z42-host`（Rust wrapper）、
> `src/toolchain/workload/test/agent/src/agent.z42`（test-agent）、
> `src/toolchain/workload/desktop/shell/testhost.c`（desktop C 壳）、
> `src/toolchain/workload/wasm/platform/src/lib.rs` + `wasm/testhost/`（浏览器面）、
> `scripts/test/xtask_test_embedded_corpus.z42`（语料枚举 / 采样 / 分片）、
> `.cargo/config.toml`（wasm shadow stack）。
>
> 平台管线与 CI 拓扑见 [跨平台测试](cross-platform.md)；runner 协议见 [测试框架机制](framework.md)。

「跑一个 z42 app」在 desktop 与 mobile 曾是两套模型：desktop 的 apphost **spawn** 一个外部 z42vm，
mobile **嵌入** z42vm（进程内）。统一到**嵌入模型**——四个平台都进程内嵌 VM——才能让一份
test-agent 与一份 app-run 代码全平台共享；而且这条嵌入路径同时是 workload 面向用户构建
跨平台 app 的地基。读这页的时机：要改嵌入入口、加一个平台的测试宿主、或者在排查
「只在设备上崩、desktop 好好的」这类问题。

## 1. 一份 app-run 核心，三个前端

```
                 z42::app::run  (app.rs) —— 唯一「跑一个 z42 app」实现
                 ╱                      ╲
        main.rs                z42-host::run_app  +  z42::host::z42_host_run_app (C ABI)
    (z42vm 二进制 CLI)          (嵌入：desktop C / Swift / JNI / wasm 壳)
```

- **`z42::app::run(file, entry, RunOpts)`** —— 完整启动序列：search_dirs → `z42.core` 预载 →
  加载 entry artifact → AOT eager BFS（否则 lazy）→ merge → 建 VmContext + lazy_loader +
  observer replay → `Vm::run`。
- **`main.rs`** 解析 CLI → 组 `RunOpts`（mode / libs_dir / program_args / print_stats）→ 调核心。
- **`z42-host::run_app` / `z42_host_run_app`(C)** 是嵌入前端，同样调核心，各自建 VmContext。
- **`RunOpts` / `default_mode`** 由调用方决定（main 读 CLI/config/build 默认；嵌入取
  "编进了 jit 就 jit，否则 interp"），核心只负责 load + run。

**三个前端互不调用，都调同一核心**——这就是"共享嵌入代码"的准确含义。

> ⚠️ **入口辨析**（踩过）：`z42-host` **crate** 的 `run_app`（Rust API）与
> `host/mod.rs` 里的 C 符号 `z42_host_run_app` 是**两条路**。所有原生嵌入壳
> （desktop C test-host / iOS Swift / Android JNI）调的是**后者**；前者只被
> `run_app_smoke` 示例用。往前者上补东西在移动端**无效**。

## 2. test-agent：on-device 的那一份 runner

`src/toolchain/workload/test/agent`（命名空间 `Z42.TestHost.Agent`）是一个薄前端，
打包成 app.zpkg，经嵌入前端跑。**一份 z42 字节码，四平台同一个 runner**——
这消除了 R1–R7 native driver 的四语言重复。

一次性命令（经 `Environment.GetCommandLineArgs()` 拿到转发的 `-- <args>`）：

```
<target.zbc|manifest.json> [format] [out-path]
   format   = "json"（默认）| "pretty" | "tap"
   out-path = 给了就把 JSON 报告写到该文件，否则打到 stdout
```

`out-path` 这一个参数就是全部的平台差异：desktop 不传 → 报告走 stdout（harness 捕获进程 stdout）；
**没有进程 stdout 的宿主**（wasm / iOS / Android）传一个 VFS 或 temp 路径，再读回来。
一份 agent、同一个 `Std.Test.Runner`，只是取报告的通道不同。

**归属**：test-agent 住在按需下载的**能力 workload** `workload/test/agent`（平台无关），
不是某个平台的东西、也不常驻 SDK 核心。平台专属的嵌入 host 壳仍住各 `workload/<plat>/`。
边界：共享 agent 随 test workload 下载，平台 host 随平台 workload 下载。

> z42b 是**构建**工具，test-agent 是**运行**工具，两者共用同一个 `Std.Test.Runner` /
> `Std.Test.BundleRunner`。

## 3. 静态 / 动态链接

嵌入 VM = 把 z42 runtime 链进原生壳。产物走**独立的 `cargo rustc --crate-type=`**
（尊重 `[lib]` rlib-only 的现状，避免主 build 出现三套 metadata 冲突）：

- **static**：`--crate-type=staticlib` → `libz42.a`。链接时**显式给 `.a` 路径**——
  macOS 上 `-lz42` 会优先挑 `.dylib`。
- **dynamic**：`--crate-type=cdylib` → `libz42.{dylib,so,dll}`，`-lz42` 解析之，
  运行期靠 `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` / rpath。

| 平台 | static | dynamic | 约束来源 |
|---|:---:|:---:|---|
| desktop | ✅ | ✅ | 真·可切换 |
| iOS | ✅ | ❌ | App Store 禁任意 dylib；xcframework 静态 |
| wasm | ✅ | ❌ | 单模块 |
| Android | ✅ | ✅ | `.so` via JNI 天然 |

开关在共享层（workload manifest `[platform.<p>] link=`）表达，平台约束在上表声明。

desktop 是这套东西的参考实现与模板：`workload/desktop/shell/testhost.c` 链 libz42
（static 或 dynamic）+ 调 `z42_host_run_app`。三前端（z42vm 二进制 / Rust `run_app` /
C `z42_host_run_app`）、两链接形态，全部产出同一份结构化 JSON。

## 4. wasm 的两点适配

wasm 没有文件系统、也没有进程 stdout，所以它**不能**直接复用 desktop 那条路。
只有两点不同，其余全共享：

**(a) 加载走 fs backend，不是 `std::fs`。** 运行期工件读取改走
`corelib::fs_backend::active()`——native 仍是 `std::fs`（字节不变），wasm 是内存 VFS。
改动点集中在 `metadata/loader.rs`（`load_zbc` / `load_zpkg` 及 `.zsym` sidecar、
indexed-zpkg 散装读、命名空间目录扫描）、`metadata/lazy_loader.rs`（`ZpkgCandidate::build`）、
`app.rs`（`z42.core` 预载的 `exists`、entry-dir 的 `is_dir`）。

于是宿主先把 agent app.zpkg + stdlib zpkg + bundle（manifest + 各 case zbc）挂进 VFS，
`z42::app::run` 就能像在磁盘上一样加载它们——**同一个 app-run 核心**。

**(b) 报告经 VFS 文件回传，不是 stdout**（即 §2 的 `out-path`）。

wasm-bindgen 面只有自足的三个函数（`workload/wasm/platform/src/lib.rs`，
与 `Z42VM` handle API 并列）：

```
mountAsset(path, bytes)                // 挂进全局内存 VFS
runTestApp(app, entry, libs, args)     // → z42::app::run（interp；wasm 无 JIT）
readAsset(path) -> bytes               // 读回报告文件
```

浏览器 harness（`workload/wasm/testhost/{index.html,run.js}`，静态、签入）：
`init()` → fetch `files.json` → 逐个 `mountAsset` → `runTestApp(agent, "", "/libs",
[manifest, "json", "/out/report.json"])` → `readAsset` → `window.__report`（Playwright 读）。

**iOS / Android 反而更简单**：它们有真文件系统，不需要 VFS——直接把 bundled corpus 引用为路径、
调现成的 `z42_host_run_app`，报告仍走 out-path 文件回传。只有原生壳与打包不同
（iOS 走 Swift 门面 + XCTest；Android 走 JNI + instrumented test）。

## 5. 语料 bundle 的三步流水线

`_buildTestBundle`（`scripts/test/xtask_test_embedded_corpus.z42`）把三个关注点解耦：

```
_enumerateCorpus(root, filter)        → _CorpusCase[]   （结构化，rid 无关）
  → _targetExcludes(rid, name) 过滤    → included[]      （平台能力门控）
  → 二选一：
       shardN > 0  → _shardCorpus(included, k, n) → selected[]   （全覆盖分片，不 cap）
       shardN == 0 → _sampleCorpus(included, cap) → selected[]   （按类 round-robin 采样）
  → 逐 selected 编译（kind → golden / unit / dir-unit）→ manifest.json
```

### 5.1 枚举是唯一 SoT

`_enumerateCorpus` 产出 `_CorpusCase` 描述符（name / bucket / kind / 源路径 / interpOnly），
覆盖四段：① `src/tests` goldens（dir 与 flat 两种布局）② stdlib `[Test]` 文件单元
③ stdlib `[Test]` 目录单元 ④ stdlib lib-goldens。
**`xtask test list`（只读 catalog）与 bundle 构建共用这一个枚举**——两个消费者、零漂移。

枚举**刻意 rid 无关**：能力门控与 cap 是 bundle 时的策略，不混进枚举，
所以 `test list --rid <rid>` 能对同一份用例集叠加任意平台视角（标为 `EXCL(<rid>)`）。

> **不变式**：枚举顺序稳定，且**同 bucket 的用例在数组里连续**。
> `_sampleCorpus` 靠这一点做零额外分配的分桶，`_shardCorpus` 靠它让每片天然跨类别均衡。
> 改枚举顺序会静默破坏这两者。

### 5.2 采样 vs 分片

`embedCap = 60`（rid 含 wasm/ios/android 时），desktop `= 0` 即不 cap、全覆盖。

- **`_sampleCorpus`（无 `--shard` 时）** —— 按 bucket **round-robin**：连续桶按轮次一桶取一例，
  直到取满 cap 或全部取尽。定位是**快速本地 / 手动 smoke**，验嵌入执行路径（agent + per-case 隔离
  + `app::run`）通不通。
- **`_shardCorpus`（`--shard k/n`）** —— **不设 cap**，取 `included[]` 里 `index % n == k-1` 的那一片。
  n 片并集 = 全集、零重叠、可复现。nightly tier-2 用它跑全覆盖。

> 采样的旧实现是「排序后取前 60」，结果字母序靠前的类别（arith / array…）挤满预算，
> 靠后的（try / string / stdlib 单元）一个都抽不到——覆盖面偏斜。round-robin 是为了修这个。
>
> **本地怎么验**：`xtask test embedded --rid iossim-arm64 [--shard k/4]` 会打印
> `bundle: N cases` 与采样/分片报告，看被抽到的 case 名跨类别分布、或确认 n 片并集=全集，
> **不需要真跑模拟器**。

## 6. 全覆盖暴露的三类问题（长期有效的教训）

60 例 smoke 只碰一个子集；全覆盖必然撞上 smoke 从没抽到的问题。三类里有两条是**架构级不变量**：

### 6.1 嵌入栈预算必须 ≥ desktop

z42 解释器**在原生调用栈上递归**——每次 z42 调用一层原生帧，没有 reify 的帧栈。
于是"desktop 能跑的有限但深的递归"在栈更小的宿主上直接炸。两个面：

- **mobile（SIGSEGV）**：嵌入 host 从**调用方线程**跑 VM，而 Android `AndroidJUnitRunner` /
  iOS XCTest 的线程栈只有约 512KB–1MB（desktop 主线程约 8MB）→ 整进程 SIGSEGV
  （logcat：`stack pointer is not in a rw map; likely due to stack overflow`），而 R1–R7 却全过。
  **修法**：C ABI 入口 `z42_host_run_app`（`src/runtime/src/host/mod.rs`）把 `z42::app::run`
  放到一条 **16 MB 大栈线程**上跑再 join。
- **wasm（OOB 陷阱）**：wasm 的 shadow stack 在 linear memory 里，**默认仅 1 MiB**。
  一个解析 300 层嵌套的用例在**还没触及解析器 256 层 DoS cap 抛异常之前**就把 shadow stack 压穿，
  `__stack_pointer` 下溢 → 下一次 local 存储越界 → OOB 陷阱。
  wasm **无栈保护页**，所以溢出表现为 OOB 而不是 "call stack exhausted"——这也是它伪装成
  "随机" OOB、console 无输出、trap 后实例即死无法回读 VFS 的原因。
  **修法**：`.cargo/config.toml` 给 `[target.wasm32-unknown-unknown]` 加
  `-C link-arg=-zstack-size=16777216`，把 shadow stack 提到 16 MiB。

**16 MB 这个数不是拍的**：崩溃用例在约 1MB 的移动端栈溢出、却在 desktop 8MB 通过 → ≥8MB 即够，
2× 留余量；再大无益（需要 >8MB 的程序在 desktop 本来就崩）。64 位下是虚拟保留、只 commit 触及页，
在移动端每线程上限内。

> 两处是同一条原则「**嵌入栈预算 ≥ desktop**」的两面：一处补 native 线程栈，一处补 wasm shadow stack。
> 新加一种嵌入宿主时先问这个问题。

### 6.2 共享 VM 里同 namespace 多文件会静默冲突

嵌入 bundle 把每个 `[Test]` **文件**单独编成一个 `.zbc`，`BundleRunner` 的 unit 段把它们
**依次 load 进同一个共享 VM**（golden 走隔离，unit 不隔离）。而原生模块加载器 `__load_module`
**按 namespace 去重（first-wins）**——第 2 个声明同一 namespace 的 `.zbc`，它的 `[Test]` 自由函数
**不再注册**，`__invoke_static` 找不到 → 该模块每个测试报 `function ... not found`，
**伪装成测试失败**。

这不是 wasm 专属，desktop embedded 也复现。`xtask test stdlib` 不踩，是因为它把一个库的测试
**一起编成一个模块**（namespace 在编译期就合并了）。

当前语料靠约定回避（每个测试文件一个独立 namespace）。**遗留**：harness 对这种情况仍会**静默**误报。
根治需要 unit 也隔离（仿 golden）或 bundler 把同 namespace 文件合编一个 `.zbc`。

### 6.3 「跑不了」要分清是能力缺口还是测试自身不可移植

见 [跨平台测试 §4.3](cross-platform.md)。误判会让排除表越滚越大且再也说不清为什么。

## 7. 在哪改

| 要改什么 | 改哪 |
|---|---|
| 启动序列 / 加载顺序 | `src/runtime/src/app.rs` |
| 嵌入入口（所有原生壳共用） | `src/runtime/src/host/mod.rs` 的 C 符号 + `src/runtime/include/z42_host.h` |
| agent 的命令面 / 报告通道 | `src/toolchain/workload/test/agent/src/agent.z42` |
| bundle 的跑法（golden 隔离 / unit 共享） | `src/libraries/z42.test/src/BundleRunner.z42` |
| 语料枚举 / 采样 / 分片 | `scripts/test/xtask_test_embedded_corpus.z42` |
| wasm 宿主面 | `src/toolchain/workload/wasm/platform/src/lib.rs` + `wasm/testhost/` |
| 栈预算 | mobile：`host/mod.rs` 的 `EMBED_STACK`；wasm：`.cargo/config.toml` 的 `-zstack-size` |
