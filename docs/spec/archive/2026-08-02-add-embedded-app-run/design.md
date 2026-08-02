# Design: 嵌入式 app 运行基座 + desktop 参考

> 定位:这不只是测试基础设施——它是**把 workload 系统补齐成面向用户的跨平台 app 开发框架**。
> `run_app` 必须是**通用**的"嵌入式跑任意 z42 app.zpkg"(用户的跨平台应用),test-agent 只是第一个消费者。

## 核心决策:抽出共享的 app-run 核心

**问题**:z42vm 二进制(`main.rs`)与嵌入式运行(`z42-host`)各有一套"跑 app"逻辑。main.rs 的
app-run 序列是 VM 核心启动路径(~250 行,intricate):
- libs 解析 + search_dirs(entry-zpkg 目录 → stdlib libs,support-colocated-zpkg-deps)
- 加载 entry 模块 + z42.core 预载 + 去重
- lazy_loader 候选集(DEPS section + namespace 反查)
- observer replay / cross-pkg impl pairs 种子
- `Vm::new(mode)` + `vm.run(ctx, entry)`

**决定**:把这段抽成 **`z42::app::run(path, entry, opts) -> Result<i32>`**(lib crate),
`main.rs`(二进制)与 `z42-host::run_app`(嵌入)**都调它**——一份 app-run 实现,服务:
- z42vm 二进制(现状)
- **嵌入式**:desktop 自包含 / wasm / ios / android(4 平台共享)
- **用户跨平台 app**(workload 打包的任意 app)+ test-agent(第一个消费者)

```
z42::app::run(path, entry, opts)        ← 唯一 app-run 实现(lib crate)
   ├── z42vm 二进制 main.rs              (behavior-preserving 重构调它)
   └── z42-host::run_app                 (嵌入 Tier-2 wrapper)
          └── z42-abi::z42_run_app       (C 符号,平台壳绑它)
```

## Architecture(嵌入式 app-run + workload)

```
用户 z42 app / test-agent (z42 源)
        │ z42c build
        ▼
     app.zpkg  ──────────────┐  workload 打包(WorkloadBase 5 相位)
        │                    │  + stdlib zpkg + (test 时)test-zbc
        ▼                    ▼
  ┌─────────────────────────────────────────────┐
  │ 平台 app(嵌入式)                             │
  │   薄原生壳(C/Swift/Kotlin/JS)               │
  │     → z42_run_app(app.zpkg, entry, argv)     │  ← z42-abi C 符号
  │        → z42-host::run_app                    │  ← Tier-2 wrapper
  │           → z42::app::run                     │  ← 共享核心
  │              → 嵌入式 z42vm(static/dynamic) │
  └─────────────────────────────────────────────┘
```

## Decisions

### D1: 抽 `z42::app::run` 共享核心(见上)
理由:避免 main.rs 与嵌入各写一套跑-app;通用化(用户 app);4 平台 + 二进制单一实现。
**约束**:重构必须 behavior-preserving——全 golden 套件是这条路的回归网,重构后必须全绿。

### D2: run_app 在 z42-host(不新开 crate)
Tier-2 wrapper 直接调 `z42::app::run`;C ABI 由 z42-abi `z42_run_app` 暴露。少 crate churn。

### D3: 静/动链接走 `cargo rustc --crate-type=`,不改 [lib]
尊重 rlib-only 现状(Cargo.toml:32-34,避免三套 metadata 冲突)。LinkMode 选 staticlib/cdylib。
**已验证**:cdylib → `libz42.dylib` 产出;staticlib → `libz42.a`(desktop test 已用)。

### D4: workload 面向用户开放
WorkloadBase 5 相位补齐后,`z42b publish --rid <platform>` = 用户构建跨平台 app 的入口
(test-agent 只是其中一个 app)。本 change 先 desktop self-contained 参考;其余平台复用。

## Implementation Notes(app-run 核心抽取步骤)

1. 新建 `src/runtime/src/app.rs`(`pub mod app` in lib.rs):`run(path, entry, opts)` + `RunOpts`
   (mode / libs_dir / argv / verbose)。把 main.rs 的 load→lazy-loader→vm.run 序列搬进来。
2. `main.rs` 重构成:解析 CLI → 组 `RunOpts` → 调 `z42::app::run` → 退出码。**行为不变**。
3. cargo build + **全 golden 套件**(`xtask test e2e`)必须全绿(核心路径回归网)。
4. `z42-host::run_app(cfg, path, entry, argv)` 调 `z42::app::run`;`z42-abi::z42_run_app` C 包装。
5. desktop shell(`workload/desktop/shell/main.rs`)链 libz42(static)+ 调 z42_run_app 跑 agent。

## Testing Strategy
- **D1 重构回归**:全 golden(interp)+ 自举不动点——app-run 路径不变的铁证。
- **嵌入 e2e**:desktop shell 嵌入跑一个 test 用例 → 结构化 JSON(static + dynamic 各一次)。
- GREEN 以 CI 为权威(冷环境本地部分验)。

## Deferred
- wasm/ios/android 落地(复用 z42::app::run + z42_run_app;各加薄壳)——后续 change。
- 完整命令通道(persistent agent)+ 结果汇总——后续 change(本 change desktop 用最简 args/stdout)。
- WorkloadBase 全 5 相位 + 用户 app 文档——workload 开放给用户的独立里程碑。
