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
