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

## 6. Deferred / 后续

- **mobile 壳**(wasm/ios/android):绑同一 `z42_host_run_app` C 符号 + 各平台打包 —— 复用本
  change 的核心 + agent,只加薄壳 + 传输。独立 change。
- **WorkloadBase 5 相位补齐**:让 `z42b publish --rid <platform>` 成为用户构建跨平台 app 的入口
  (test-agent 只是其中一个 app)。workload 面向用户的里程碑。
- **接入 xtask test platform**:用嵌入式 agent 取代 R1–R7 的 4 语言 native driver。
- **完整命令通道**(persistent agent)+ 结果汇总(统一 schema → 单 GitHub Check)。
