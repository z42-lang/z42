# 嵌入式 app 运行 + 共享测试宿主

> 对齐:2026-08-02(add-embedded-app-run)。
> 实现:`src/runtime/src/app.rs`(核心)、`src/runtime/src/host/mod.rs`(C 符号)、
> `src/runtime/crates/z42-host`(Rust wrapper)、`src/toolchain/testhost/agent`(test-agent)、
> `src/toolchain/workload/desktop/shell/testhost.c`(desktop C 壳)。

## 1. 动机

跑测试用例(以及跑用户跨平台 app)在 desktop 与 mobile 曾是两套模型:desktop apphost **spawn**
外部 z42vm(`z42-hostrun`,框架依赖);mobile **嵌入** z42vm(`z42-host`,进程内)。统一到
**嵌入模型**——4 平台都进程内嵌 VM——才能:一份 test-agent + 一份 app-run 代码全平台共享,
且这条嵌入路径同时是 **workload 面向用户构建跨平台 app** 的地基。

## 2. 一份 app-run 核心,三个前端

```
                 z42::app::run   (app.rs) —— 唯一"跑一个 z42 app.zpkg"实现
                 ╱            ╲       load app+deps → merge → VmContext → vm.run
        main.rs                z42-host::run_app  +  z42::host::z42_host_run_app (C ABI)
     (z42vm 二进制 CLI)         (嵌入:desktop C / Swift / JNI / wasm 壳)
```

- **`z42::app::run(file, entry, RunOpts)`**:从 z42vm 二进制 main.rs 抽出的核心启动序列
  (search_dirs → z42.core 预载 → 加载 entry artifact → AOT eager BFS / 否则 lazy → merge →
  VmContext + lazy_loader + observer replay → `Vm::run`)。behavior-preserving,全 golden 兜回归。
- **main.rs**:解析 CLI → 组 `RunOpts`(mode/libs_dir/program_args/print_stats)→ 调核心。
- **z42-host::run_app** / **z42_host_run_app(C)**:嵌入前端,同样调核心,自足(各建 VmContext)。
- **RunOpts / default_mode**:调用方解析执行模式(main:CLI/config/build 默认;嵌入:default_mode
  = jit if compiled else interp),核心只负责 load+run。

**互不调用,都调同一核心** —— 这就是"共享嵌入代码"。

## 3. 共享 test-agent(on-device 测试运行器)

`src/toolchain/testhost/agent`(`Z42.TestHost.Agent.Main`):收命令 `<target.zbc> [format]`
(经 `GetCommandLineArgs` = 转发的 `-- <args>`)→ `Std.Test.Runner.RunModule` → 结构化报告
(json → 一个 JSON 对象到 stdout)。**一份 z42 字节码,4 平台同一 runner**——消除 R1–R7 driver
4 语言重复。z42b 是构建工具,test-agent 是运行工具,共用同一 `Std.Test.Runner`。

打包为 app.zpkg,经嵌入前端跑:`run_app(agent.zpkg, entry=None, args=[target.zbc, format])`。

## 4. 静态 / 动态链接(嵌入 VM 的两种形态)

嵌入 VM = 把 z42 runtime 链进原生壳。产物走**独立 `cargo rustc --crate-type=`**(尊重
`[lib]` rlib-only 现状,避免主 build 三套 metadata 冲突,Cargo.toml:32-34):

- **static**:`--crate-type=staticlib` → `libz42.a`。链接**显式给 .a 路径**(macOS `-lz42` 会优先
  选 `.dylib`)。
- **dynamic**:`--crate-type=cdylib` → `libz42.{dylib,so,dll}`。`-lz42` 解析之 + 运行期
  `DYLD_LIBRARY_PATH`/`LD_LIBRARY_PATH`/rpath。

平台约束矩阵:

| 平台 | static | dynamic | 备注 |
|------|:---:|:---:|------|
| desktop | ✅ | ✅ | 真·可切换 |
| ios | ✅ | ❌ | App Store 禁任意 dylib;xcframework 静态 |
| wasm | ✅ | ❌ | 单模块 |
| android | ✅ | ✅ | .so via JNI 天然 |

开关在共享层(workload manifest `[platform.<p>] link=`)表达,平台约束在矩阵声明。

## 5. desktop 参考(端到端已验证)

`workload/desktop/shell/testhost.c`:链 libz42(static 或 dynamic)+ 调 `z42_host_run_app`
嵌入跑 app.zpkg。实测(macOS arm64):

```
z42c build agent → z42.testagent.zpkg
cc testhost.c libz42.a  (static)  → ./testhost agent.zpkg sample_tests.zbc json → {"summary":{"total":2,"passed":2,…}}
cc testhost.c -lz42     (dynamic) → 同上 JSON
z42-host/examples/run_app_smoke (Rust 嵌入)                                     → 同上 JSON
```

三前端(z42vm 二进制 / Rust run_app / C z42_host_run_app)、两链接(static/dynamic)全跑通,
出同一结构化 JSON —— desktop 自包含嵌入,与 mobile 同模型,是其模板。

## 5.5 wasm 嵌入 test-host(add-wasm-testhost G6)

wasm 没有文件系统、也没有进程 stdout,所以它**不能**直接复用 desktop 的
`z42_host_run_app`(std::fs)+ stdout 捕获。两点适配,其余全共享:

**(a) 加载走 fs backend(不是 std::fs)。** 运行期工件读取原本硬编 `std::fs`,现改走
`corelib::fs_backend::active()`——native 仍是 `std::fs`(字节不变),wasm 是内存 VFS。改动点:

- `metadata/loader.rs`:`load_zbc` / `load_zpkg`(主体 + `.zsym` sidecar)、indexed-zpkg 散装读、
  `find_namespace_in_{zbc,zpkg}_dirs`(命名空间目录扫描)。
- `metadata/lazy_loader.rs`:`ZpkgCandidate::build`(读 zpkg meta)、`build_in_dirs`(is_file)。
- `app.rs`:`z42.core` 预载的 `exists`、entry-dir 的 `is_dir`(`metadata`/`canonicalize` 只喂
  replay 字节数 / 符号链接去重,缺失即优雅降级,wasm 无需改)。

于是宿主先把 agent app.zpkg + stdlib zpkgs + bundle(manifest + 各 case zbc)`mountAsset` 进
VFS,`z42::app::run` 就能像在磁盘上一样加载它们——**同一 app-run 核心**。

**(b) 报告经 VFS 文件回传(不是 stdout)。** test-agent 收可选第 3 个参数 `out-path`:给了就把
JSON 报告 `File.WriteAllText` 到该文件,否则 `Console.WriteLine`。desktop 不传→stdout(不变);
wasm/mobile 传一个 VFS 路径,再 `readAsset` 读回。**一份 agent,全平台同一 runner**,只是取报告
的通道不同。

wasm-bindgen 面(`workload/wasm/platform/src/lib.rs`,与 `Z42VM` handle API 并列的自足 3 函数):

```
mountAsset(path, bytes)     // 挂进全局内存 VFS(app.zpkg / zpkg / zbc / manifest)
runTestApp(app, entry, libs, args)   // → z42::app::run(interp;wasm 无 JIT)
readAsset(path) -> bytes    // 读回报告文件
```

浏览器 harness(`workload/wasm/testhost/{index.html,run.js}`,静态、签入):`init()` → fetch
`files.json` → 逐个 `mountAsset` → `runTestApp(agent, "", "/libs", [manifest,"json","/out/report.json"])`
→ `readAsset` → `window.__report`(Playwright/CI 读)。

构建:`xtask test embedded --rid browser-wasm [--filter …]` —— 复用 desktop 的 bundle builder +
test-agent,加 `wasm-pack build` + 资产装配(pkg + harness + app/libs/bundle + files.json)到
`artifacts/build/wasm-test/`。**编译已本地验证**(native `cargo check` + `wasm-pack build` 全绿);
**RUN 是浏览器/Playwright 门(CI)**——本机无 node/浏览器不跑,与冷启动路径同理以 CI 为准。

## 5.6 iOS / Android 嵌入 test-host(add-wasm-testhost G6)

iOS/Android **有真文件系统**,比 wasm 简单——不需要 VFS,直接把 bundled corpus 引用为路径、调
**现成的 `z42_host_run_app`**(std::fs),报告仍走 out-path 文件回传。只有原生壳 + 打包不同:

- **公有头**:`z42_host_run_app` 补进 `src/runtime/include/z42_host.h`(iOS modulemap / Android
  JNI 转发头都指向它),不再靠各壳 extern。
- **iOS**:Swift 门面 `Z42TestHost.runApp`(marshal argv → C 符号);嵌入 XCTest
  `Z42EmbeddedTests.testEmbeddedBundle`(bundled agent+libs+bundle 作 `Resources/embedded/`,报告
  写 temp 文件后断言 `failed==0`),随 `Z42VM` scheme 走既有 `xcodebuild test`。
  `xtask test embedded --rid ios-arm64|iossim-arm64`:装配 Resources + 建 xcframework(rid 选
  deploy slice,host slice 恒建以便 `swift test`)。
- **Android**:JNI `Java_..._Z42TestHost_nativeRunApp`(z42vm_jni.c)+ Kotlin `Z42TestHost.runApp`
  + instrumented test `Z42EmbeddedInstrumentedTest`(assets `embedded/` 拷到 cacheDir → 路径调用 →
  读报告)。`xtask test embedded --rid android-arm64|android-x64`:装配 androidTest assets +
  cargo-ndk 建 `libz42_platform_android.so`(rid 选 ABI)。

> RID 与打包链一致(`_ridCategory`/`_ridToCargo`,packaging 复用):arch 后缀选实际 slice/ABI,
> 故 `--rid android-x64` 只编 x86_64、`--rid iossim-arm64` 只编 sim slice——CI 按需单编,不浪费。
> 省略 `--rid`(或桌面 RID)= 本机进程内跑。

**验证**:
- iOS:staticlib 三 slice + xcframework + `swift build --build-tests` 编过,**`swift test` 在
  macOS host 实跑 `testEmbeddedBundle` PASSED**(端到端;模拟器/设备同一二进制,CI 门)。
- Android:cargo-ndk 建 `.so`(arm64-v8a + x86_64)全绿(fs_backend 改动跨编 android 通过);JNI C
  经 NDK clang 语法校验通过。AAR(gradle)+ emulator RUN(`connectedAndroidTest`)为 CI/emulator 门。

## 5.7 嵌入 corpus 枚举 + 能力门控 + 按类 smoke 采样(tidy-test-system, 2026-08-19)

嵌入 bundle 的构建(`_buildTestBundle`, `scripts/test/xtask_test_embedded.z42`)分三步流水线,
枚举、门控、采样三个关注点解耦:

```
_enumerateCorpus(root, filter)      → _CorpusCase[]   (结构化,rid 无关)
  → _targetExcludes(rid, name) 过滤 → included[]      (平台能力门控)
  → 选择 (二选一):
      · shardN>0  → _shardCorpus(included, k, n) → selected[]  (全覆盖分片,不 cap)
      · shardN==0 → _sampleCorpus(included, cap) → selected[]  (按类 round-robin smoke 采样)
  → 逐 selected 编译 (kind → _goldenEntry / _emitUnitZbc / _emitDirUnit) → manifest.json
```

### 枚举 SoT:`_enumerateCorpus`

单一枚举函数产出 `_CorpusCase` 描述符(name / bucket / kind / 源路径 / interpOnly),覆盖四段:
① src/tests goldens(dir + flat 两布局)② stdlib `[Test]` 文件单元 ③ stdlib `[Test]` 目录单元
④ stdlib lib-goldens。**`test list` 命令(catalog)与 bundle 构建共用这一个枚举**——两个消费者、
零漂移。枚举**rid 无关**:能力门控与 cap 是 bundle 时策略,不混进枚举,故 `test list` 能对同一份
用例集叠加任意 `--rid` 视角。

关键不变式:枚举顺序稳定(cats 字母序 → 每 lib 内 单元→目录单元→lib-golden),因此**同 bucket
的用例在数组里连续** —— `_sampleCorpus` 依赖这一点做零额外分配的分桶。

### 能力门控:`_targetExcludes`

粗粒度(库级)判定:目标平台缺某能力(wasm 无 socket/threads/native-fs/env/process)则该用例整例
排除,让平台 corpus 诚实(不跑注定崩的 socket/stream 测)。native 目标(desktop/ios/android)跑全集。
`test list --rid <rid>` 把此门控标为 `EXCL(<rid>)`,是「哪个平台能跑哪些用例」的可查视图。

### 按类 smoke 采样:`_sampleCorpus`(本次核心改动)

受限平台(wasm/ios/android)的完整 corpus(~500 例)在浏览器 interp / 模拟器 / 真机上跑 >20min
(wasm 曾超时、android 曾撞 60min job 上限),故 cap 为 smoke 子集(`embedCap=60`,desktop=0 不 cap
= 全覆盖的家)。**旧实现是「排序后前 60」——字母序靠前的类别(arith/array…)挤满预算,靠后的
(try/string/stdlib 单元)一个抽不到,覆盖面偏斜。**

新实现按 bucket **round-robin**:把连续桶(枚举顺序天然连续)按轮次一桶取一例,直到取满 cap 或全部
取尽 —— 每个类别都拿到代表 case,60 的预算在类别间均摊。确定性:桶序 = 首次出现序(即字母序)、
桶内序 = 枚举序、结果按原枚举序发射(manifest 顺序稳定)。cap≤0 或 total≤cap → 返回全集(desktop
逐字节不变)。

> 采样**定位是无 `--shard` 时的快速本地/手动 smoke**(验嵌入执行路径 agent + per-case 隔离 +
> `app::run` 通不通)。tier-2 的 nightly 全覆盖改由下方分片承担(tier2-shard-full-coverage);
> 采样路径本身保持不变,仍是 desktop-字节不变的 smoke 家。
> 本地验采样分布:`xtask test embedded --rid iossim-arm64` 打印 `bundle: N cases` + 采样报告,
> 看被抽到的 case 名跨类别分布即可,无需真跑模拟器。

### 全覆盖分片:`_shardCorpus` + tier-2 matrix(tier2-shard-full-coverage, 2026-08-19)

smoke 采样只覆盖 ~14%,不足以在受限平台上真正验语料。全覆盖分片让 nightly 把**全部可跑用例**跑完:
`test embedded --rid <tier2> --shard k/n` 时,`_buildTestBundle` 走 `_shardCorpus(included, k, n)`
而非 `_sampleCorpus` —— **不设 cap**,只取"能力门控后"的 `included[]` 里 `index % n == k-1` 的那
一片。n 个 shard 的并集 = 全集,零重叠(纯 index 取模分区),按枚举序确定、可复现。`index%n` 在
`_enumerateCorpus` 的连续 bucket 上交错,故每片天然跨类别均衡。`--shard` 复用 golden/stdlib 的
`_parseShard`([0,0] = 不分片 → 回到 smoke 采样)。

**为何分片而非提 cap**:cap 由**单 job 时间墙**(wasm Playwright / android 60min emulator)封顶,
提 cap 会撞墙;分片把 corpus 的**编译+跑**摊到 n 个平行 CI runner,每片只跑 1/n,墙不动、覆盖到 100%。

**落地范围**:`test-wasm`(tier2-shard-full-coverage, n=3)**与 `test-ios` / `test-android`
(tier2-mobile-coverage, n=3)三者都走全覆盖分片**。mobile 分片打开时同步补齐了它**独立的能力
排除审计**(见 §5.8)——mobile 的能力集与 wasm **不同**,不能照搬 wasm 的排除表,故需独立一节。

**T1 拓扑的代价(已知、可测)**:矩阵每片是独立 runner,各自重付一遍平台冷构建。wasm 的 R1–R7
(`test platform wasm`,与语料无关)因此只在 `shard==1` 跑。wasm 语料随 stdlib 增长(排除能力用例后
~340 例),Playwright 里 interp 每例(fresh VmContext + stdlib reload,单线程 wasm)其实不轻——**单片
整体在浏览器内跑已超 10min**,一度撞穿 `playwright.embedded.config.ts` 旧的 620s(10.3min)whole-test
timeout(shard 2 确定性红,`Test timeout 620000ms exceeded`)。**fix-wasm-shard-timeout 把该 timeout
提到 25min(inner waitForFunction 24min)、job 墙 60→75min**——冷构建 ~27min + 25min 跑仍稳在 75min 内。
选提 timeout 而非提 n:每加一片都要重付一遍冷 wasm-pack 构建,提 timeout 零额外 job。junit/artifact
按 shard 命名(`junit-wasm` 仅 shard 1 出 R1–R7)。若单片整体跑逼近墙(语料再涨),再升级 T2(冷构建
产物只建一次 + 分片只跑,仿 share-goldens-no-regen)或提 n 分更细。

本地验分片切分:`xtask test embedded --rid iossim-arm64 --shard 1/4`(及 2/4…)看 `embed shard k/n`
报告的 selected 数,确认 n 片并集=全集、无重叠——无需真跑模拟器。

### 全覆盖暴露的四类问题(前三类本 PR 修复,第四类转 follow-up,tier2-shard-full-coverage)

60-smoke 只碰一个子集;全覆盖必然撞上 smoke 从没抽到的问题。逐轮 dispatch 暴露:①②③ 已修(wasm
故因此本 PR 全覆盖绿),④ 是 mobile 全覆盖的门槛、转独立 follow-up(故本 PR mobile 仍留 smoke):

1. **wasm 能力缺口(panic + 断言失败)**。① `z42.compression` 是纯
   `[Native(lib="z42_compression")]` facade(brotli/gzip/zstd/zip/lz4/tar):wasm 无 dlopen → ext
   注册表空 → 元数据 resolver 加载即 panic(`unknown builtin __brotli_compress`);它是唯一的
   native-ext facade 库(crypto 走静态 `BUILTINS`,wasm 有)。② 排除 compression 后又暴露一批**断言
   失败**——wasm 无真文件系统 / 进程 / OS 熵 / 系统时钟,故 `z42.io` 的 process*/file*/directory*/
   path_glob*/console*/env*/gc_heap_snapshot、`z42.crypto/secure_random*`、`z42.time/datetime`
   (`DateTime.UtcNow`)物理跑不了。修:`_targetExcludes` 整桶排除 compression + 按**能力前缀**排除
   上述 io/crypto/time 用例(源码审计,非 CI 采样,更全;同 lib 的纯 string/stream-memory/hash/
   date-parse 用例仍跑),与 net/threading 同款能力门控。

2. **mobile(android+ios):嵌入运行原生栈溢出 SIGSEGV**。z42 解释器**在原生调用栈上递归**
   (每次 z42 调用一层原生帧,无 reify 帧栈)。嵌入 host 经 `z42_host::run_app` 从**调用方线程**跑 VM,
   而 Android `AndroidJUnitRunner` / iOS XCTest 线程栈只 ~512KB–1MB(desktop 主线程 ~8MB)。于是
   desktop 能跑的**有限但深**递归用例在移动端溢出 → 整进程 SIGSEGV(logcat:`stack pointer is not in
   a rw map; likely due to stack overflow`),R1–R7 却全过。修:**C-ABI 入口 `z42_host_run_app`
   (`src/runtime/src/host/mod.rs`,所有原生嵌入 shell 的唯一 C 符号:desktop C test-host /
   iOS Swift / Android JNI 都调它)把 `z42::app::run` 放到一条 16MB 大栈线程上跑再 join**,
   让嵌入栈预算 ≥ desktop。⚠️注意入口辨析:z42-host **crate** 的 `run_app`(Rust API,仅
   `run_app_smoke` 示例用)是另一条路,不在移动端调用链上——补在那里无效,必须补 host/mod.rs 的
   C 符号。wasm 单线程、走 `z42_wasm` 另一入口,`#[cfg(not(wasm32))]` 门控不触及。16MB = desktop
   主线程 8MB 的 2×:崩溃用例在 ~1MB 移动端栈溢出、却在 desktop 8MB 通过 → ≥8MB 即够,2× 留余量;
   再大无益(需 >8MB 的程序在 desktop 本就崩)。64 位虚拟保留、只 commit 触及页,在移动端每线程上限内。

3. **全平台:同 namespace 多文件在共享 VM 里冲突**(不是 wasm 专属,desktop embedded 也复现)。嵌入
   bundle 把每个 `[Test]` **文件**单独编成一个 `.zbc`,`_runBundleReport` 的 unit 段把它们**依次
   load 进同一个共享 VM**(goldens 走 `__run_goldens_isolated` 各自隔离,units 不隔离)。而原生模块
   加载器 `__load_module` **按 namespace 去重(first-wins)**——第 2 个声明同一 namespace 的 `.zbc`
   其 `[Test]` 自由函数**不再注册**,`__invoke_static` 找不到 → 该模块每个测试报
   `function ... not found`(伪装成测试失败)。`test stdlib` 不踩因它把一个 lib 的测试**一起编成一个
   模块**(namespace 编译期合并)。全库仅 `z42.collections` 的 `Z42CollectionsTests` 被 3 个文件
   (linkedlist/queue/stack)共享——**唯一**触发点(其余测试文件都已是 per-file namespace)。修:给这
   3 个文件各自独立 namespace(`Z42Collections{LinkedList,Queue,Stack}Tests`),与既有约定一致,且
   无论分片如何分布都不冲突(比隔离 units 的改动更小、更稳)。
   > 遗留(harness 加固,可选 follow-up):共享 VM 对「同 namespace 多 `.zbc`」仍会**静默**误报,
   > 未来有人再写同 namespace 的测试文件会踩。根治需 units 隔离(仿 goldens)或 bundler 把同
   > namespace 文件合编一个 `.zbc`;当前语料无触发,故先按约定回避。

4. **mobile 沙箱能力缺口(tier2-mobile-coverage 处理中,见 §5.8)**。修完 ①②③ 后,mobile
   全覆盖 dispatch 显示:栈溢出没了(② 生效,logcat 无 SIGSEGV),但 **android 模拟器 / iOS 模拟器的
   app 沙箱**跑不了一大批用例——**几乎整个 `z42.net`**(socket 绑定/监听、UDP loopback/multicast、
   websocket、HTTP server:沙箱禁 bind/listen/loopback)+ **部分 `z42.io`**(process*/console/env/
   平台身份:进程 fork/exec 与可变 env 受限)。即 **mobile 不是「native 什么都能跑」**,其沙箱受限,
   只是可跑集**与 wasm 不同**(mobile 有 threads / compression / OS 熵 / 系统时钟 / 可写沙箱 fs,
   缺 net / 进程 / TTY / 可变 env)。故 mobile 全覆盖需一套**独立的能力排除审计**——即 §5.8 的
   `_targetExcludes` mobile 分支 + `test-ios`/`test-android` 分片 matrix(tier2-mobile-coverage)。

5. **wasm:深递归压穿 shadow stack → OOB(fix-wasm-yaml-deep-recursion-oob,follow-up #1,已修)**。
   tier2-shard-full-coverage 交付时 shard 2/3 留了个 `RuntimeError: memory access out of bounds`
   (str_meta 串味修复后**仍**复现,故是独立 bug),二分定位到单个用例 `z42.yaml/parse_errors`
   (孤立跑即崩、确定复现)。根因**与 ②(mobile SIGSEGV)同源**——z42 解释器在**原生栈**递归
   (每层 z42 调用一层原生帧),而 wasm 的 **shadow stack(在 linear memory 内)默认仅 1 MiB**,
   远小于 desktop 主线程 ~8MB / mobile 16MB 大栈线程。该用例 `test_deep_flow_nesting_rejected`
   解析 300 层 `[[[…]]]`(> 解析器 256 层 DoS cap):递归在**尚未触及 256 cap 抛 YamlException 之前**
   就把 1 MiB shadow stack 压穿,`__stack_pointer` 下溢 → 下一次 local 存储越界 → OOB 陷阱
   (wasm **无栈保护页**,溢出即 OOB,不是 `call stack exhausted`;这也是它伪装成"随机" OOB、
   `console` 无输出、trap 后 wasm 实例即死无法回读 VFS 的原因)。修:`.cargo/config.toml` 给
   `[target.wasm32-unknown-unknown]` 加 `-C link-arg=-zstack-size=16777216`,把 shadow stack
   提到 **16 MiB(与 ② mobile 大栈线程同预算)**,让 256 cap 可达、按设计抛 YamlException 而非陷阱。
   **② 与 ⑤ 是同一原则「嵌入栈预算 ≥ desktop」的两面**:② 补 native 线程栈(host/mod.rs 16MB 线程),
   ⑤ 补 wasm shadow stack(link-arg)。验证:`parse_errors` 单例由 CRASH→PASS,shard 2/3 全 137 例
   本地 Playwright 全绿(shard 1/3、3/3 早已绿,栈只增不减,不回归)。

## 5.8 mobile 全覆盖:能力模型 + 分片 matrix(tier2-mobile-coverage)

wasm 全覆盖(§5.7)交付后,mobile(iOS 模拟器 / Android 模拟器)由 60-smoke 升级到全覆盖分片。
关键洞察:**mobile 的能力集与 wasm 不同,不能照搬 wasm 的排除表**,故 `_targetExcludes` 分两层。

### 为什么 mobile ≠ wasm

wasm 没有任何原生设施;mobile 跑在**真原生 runtime** 上,有 wasm 缺的一大批能力:

| 能力 | wasm | mobile(iOS sim / android emu) |
|------|:----:|:----:|
| 线程(pthreads) | ✗ | ✓ |
| 原生扩展(compression 静态链接) | ✗(无 dlopen) | ✓ |
| OS 熵(secure_random) | ✗ | ✓ |
| 系统时钟(DateTime.UtcNow) | ✗ | ✓ |
| 可写文件系统 | ✗ | ✓(app 沙箱 tmp/Documents) |
| server/loopback socket + DNS | ✗ | ✗(沙箱禁 bind/listen;CI 无外网) |
| 进程 fork/exec | ✗ | ✗(iOS 禁,android app 不能 exec) |
| TTY / console | ✗ | ✗(测试 harness 无 tty) |
| 可变 env / 桌面 OS 身份 | ✗ | ✗(env 受限,身份是 "ios"/"android") |

所以 mobile **保留** wasm 必须丢的 threading / compression / secure_random / datetime / fs / stream,
只**排除沙箱真缺**的那一小撮。

### `_targetExcludes` 两层结构(`scripts/test/xtask_test_embedded.z42`)

```
desktop（非 wasm/非 mobile）        → 全跑，return false
SHARED 缺口（wasm 与 mobile 都缺）  → net/* · io/process* · io/console* · io/env* ·
                                      io/ansi_color · io/operating_system · io/platform ·
                                      cli/cli_env_fallback_and_mutex
WASM-ONLY 缺口（仅 if(isWasm)）     → threading/* · compression/* · *stream* · io/file* ·
                                      io/directory* · io/path_glob* · io/gc_heap_snapshot ·
                                      crypto/secure_random* · time/datetime
```

结果(533 例语料):wasm 排除 **121** → 可跑 412;mobile 排除 **68** → 可跑 **465**
(mobile 比 wasm 多跑 53 例:13 threading + 11 compression + 22 io fs/stream + 熵/时钟等)。

> **mobile fs/stream 的 KEEP-IN 是能力假设,靠 tier-2 CI 分片验证**:app 沙箱**有**可写 tmp,
> 故 `file_temp`/`directory_temp`/memory+file stream 假定可跑、保留在语料内。若某例实际需要沙箱
> 拒绝的东西(如硬链接、无写权限的固定路径),CI dispatch 会把它显成红,按证据加 mobile/android
> 排除。**全覆盖的原则是「揭真实平台差异、按证据收敛」,不是「先排干净求绿」**——与 §5.7 一脉相承。

### 首轮 discovery dispatch 结果(run 32422098038):iOS 全绿,android 6 例 fs 缺口

三分片 dispatch(`gh workflow run ci.yml --ref tier2-mobile-coverage`):

- **iOS(iossim)3 片全绿** —— 上表 KEEP-IN 假设对 iOS **全部成立**(threads / compression / 熵 /
  时钟 / 沙箱 fs / stream 都跑通)。排除 68 → 可跑 465。
- **android(emulator)首轮 3 片红在 6 个 `z42.io` case**(23 个 sub-test),经 fix-mobile-tmp-portability
  收敛为**两个 android-only 排除 + 4 例转可移植**:
  - **① 真能力缺口(永久 android 排除)**:`file_chmod_link_size` —— `File.Link`(硬链接)在 android
    app 沙箱 `Permission denied`(symlink 却可以)。该测试**已**用 `File.CreateTempDir`,故非 /tmp 问题、
    改不了 → **永久 android-only 排除**(iOS 沙箱能跑,**不上移到 SHARED**)。
  - **② 内存缺口(永久 android 排除,非 /tmp)**:`gc_heap_snapshot` —— `GC.WriteHeapSnapshot` 把**整个活
    堆**序列化成 V8 JSON 串(+`ReadAllText` 再读回)。嵌入 runner 里 [Test] units **共享一个 VM**,此例
    在片内偏后跑(idx ~337/464,约第 112 个 unit)→ 累积堆已很大 → 快照膨胀到**数 GB**。android emulator
    内存紧 → **low-memory-killer SIGKILL**(dispatch 32435384808:android shard 2 ~2.4GB RSS + 2.2GB swap
    → LMK;去掉此例的 shard 1/3 全绿)。iOS-sim(macOS 宿主内存充足)跑得过、desktop 非共享-VM,故
    **只在 android 排除**;wasm 本就排除它。/tmp→CreateTempDir 可移植性修复对该测试本身仍生效。
  - **③ 测试可移植性缺陷(已修,4 例转可移植)**:`directory` / `directory_copy` / `file_extras` /
    `file_last_write_time` —— 原**硬编码 `/tmp/…`** 写盘路径。iOS-sim(跑在 macOS)与 desktop 有可写
    `/tmp` 故过;**android emulator 无可写 `/tmp`** → `Read-only file system (os error 30)` / `No such
    file`。android **有**可写 temp(`File.CreateTempDir`,`file_temp` 在 android 已过)。**已把这 4 个测试
    从硬编码 `/tmp` 改用 `File.CreateTempDir`(可移植,android+iOS+desktop 都过)并删掉它们的 android
    临时排除** → android 全覆盖三片全绿(①② 两例仍 android-only 排除)。

### 分片 matrix(`test-ios` / `test-android`,n=3)

与 `test-wasm` 同构:`strategy.matrix.shard: [1,2,3]`,每片 `test embedded --rid <mobile> --shard k/3`
取全覆盖 1/3 切片(`_shardCorpus`,不 cap),三片并集 = 100%。为何必须分片:mobile 可跑集(~465)
远大于 wasm(~412),**未 cap 的全语料曾撞 60min emulator/sim 墙**(正是 60-smoke cap 的由来);
n=3 让每片 ~155 例稳在墙内。

**与 wasm 分片的差异**:wasm 的 R1–R7(`test platform wasm`)是独立 Playwright 步,只在 shard 1 跑;
mobile 的嵌入 corpus **折叠进 R1–R7 的同一次 `xcodebuild test` / `connectedAndroidTest`**(单次
sim/emulator 启动,§6),无法把 R1–R7 从 scheme 里摘出,故 R1–R7 在每片**冗余重跑**——相对每片的
boot+corpus 成本可忽略,换来「不二次启动模拟器」。junit / logcat / crash-diagnostics artifact 均按
`${{ matrix.shard }}` 命名避免矩阵内碰撞。代价同 §5.7 T1:每片独立 runner 重付一遍平台冷构建
(iOS 冷编 rust + xcframework、android 冷编 cargo-ndk + AAR),AVD snapshot 缓存 key 与语料无关、
跨片共享。

本地无法可靠验 mobile 全跑(iOS 模拟器需 xcodebuild + 冷编 rust,android 需 emulator;且 libffi-sys
的 iOS-sim 交叉编译在部分本机环境会卡),故 mobile 长尾**以 CI dispatch 为准**(与 wasm 首轮同法)。

## 6. CI 集成(add-wasm-testhost G6 —— 折叠进现有平台 job,不新增)

嵌入式 RUN **折叠进现有 `test-{wasm,ios,android}` job**(`.github/workflows/ci.yml`),复用同一
runner + 工具链 + **单次 emulator/sim 启动**——不新开并行 job(避免重复 setup / 二次 emulator 启动)。
R1–R7(嵌入 API 契约:错误码/句柄生命周期)**保留**,与嵌入 corpus 测的是不同表面,不冗余。

| job | 折叠方式 |
|-----|---------|
| test-wasm | `test platform wasm`(R1–R7 Playwright)后加 `test embedded --rid browser-wasm` + 独立 `playwright.embedded.config.ts` 跑 deployable(读 `window.__report`) |
| test-ios | 一次 sim 跑双份:`test embedded --rid iossim-arm64`(建 xcframework sim+host + 装配 Resources/embedded)→ `test platform ios assets`(R1–R7 资产)→ `test platform ios run`(**单个 `xcodebuild test -scheme Z42VM`** 同时跑 Z42VMTests + Z42EmbeddedTests) |
| test-android | `test platform android build/assets` 后加 `test embedded --rid android-x64`(x86_64 .so 配 emulator ABI + androidTest assets),**同一个 `connectedAndroidTest`** 跑 R1–R7 + Z42EmbeddedInstrumentedTest |

paths-filter 的 `platform` 组已含 `src/toolchain/workload/**`,另补 `xtask_test_embedded.z42` +
`src/toolchain/testhost/**`。

## 7. Deferred / 后续

- **接入 desktop CI**:desktop 嵌入(`test embedded` 无 rid)本地已跑,尚未挂进 test-desktop job。
- **完整命令通道**(persistent agent)+ 结果汇总(统一 schema → 单 GitHub Check)。
- **接入 xtask test platform**:用嵌入式 agent 取代 R1–R7 的 4 语言 native driver。
- **WorkloadBase 5 相位补齐**:让 `z42b publish --rid <rid>` 成为用户构建跨平台 app 的入口
  (test-agent 只是其中一个 app)。workload 面向用户的里程碑。
