# Proposal: 嵌入式 app 运行基座 + 跨平台测试宿主(desktop 参考)

## Why

Tier-2 平台自动化测试要"把测试用例跑起来",需要先有**构建测试 app**的能力。现状:
- `z42b publish` 只支持 desktop apphost(spawn 外部 z42vm,`z42-hostrun`);wasm/ios/android
  deployable = "B5 not implemented";WorkloadBase 4 平台子类全 **parked**;on-device run 胶水 TODO。
- desktop 与 mobile **两套模型**:desktop apphost = spawn z42vm(框架依赖);mobile = 嵌入 z42vm
  (`z42-host` 进程内)。测试用例要跑,需要**统一到嵌入模型**——4 平台(含 desktop 自包含)都
  进程内嵌 z42vm + 跑 `Std.Test.Runner`,才能最大化共享 test-agent + 嵌入代码。
- R1–R7 测试 driver 在 4 语言各重写(r1_r7.c/host.js/*.swift/*.kt)——最大重复。

## What Changes(本 change 只做「嵌入式 app 运行基座 + desktop 参考」)

1. **静/动产物走独立 `cargo rustc --crate-type=`**(尊重现状:`[lib]` 刻意 rlib-only,避免主
   build 三套 metadata 冲突——见 Cargo.toml:32-34;desktop test 已用此法产 staticlib)。**不改
   `[lib]`**;LinkMode 开关选 `--crate-type=staticlib`(libz42.a)或 `--crate-type=cdylib`
   (libz42.{dylib,so,dll})。
2. **共享嵌入 run-entry**:`z42-host` 加 `run_app(...)`(load app.zpkg → resolve entry → invoke
   → exit code),`z42-abi` 暴露成 C 符号 `z42_run_app(...)`。这是 4 平台共享的"跑 app"逻辑。
3. **z42 test-agent(共享字节码)**:`src/toolchain/testhost/agent/` —— Main = 收命令 →
   `Std.Test.Runner.RunModule` → 结果 JSON。一份 z42,全平台用。
4. **desktop 参考实现**:desktop shell(C/Rust main)链 `libz42`(静/动可切)+ 调 `z42_run_app`
   跑 test-agent → 结构化结果。端到端证明"嵌入式 app 构建+运行"这条最短链。
5. **静/动链接开关落点**:`workload/shared/LinkMode.z42` + manifest `[platform.<p>] link=`;
   平台约束矩阵(ios/wasm 仅 static;desktop/android 可切)。本 change 先在 desktop 打通 static,
   dynamic 作为可切选项验证。

## Scope(允许改动的文件)

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/crates/z42-host/src/lib.rs` | MODIFY | 加 `run_app(cfg, app_zpkg, entry, argv) -> Result<i32>` |
| `src/runtime/crates/z42-abi/src/lib.rs` | MODIFY | 加 `z42_run_app(...)` C 符号 |
| `src/toolchain/testhost/agent/src/agent.z42` | NEW | z42 test-agent:命令 → Std.Test.Runner → JSON |
| `src/toolchain/testhost/agent/z42.testagent.z42.toml` | NEW | agent app 工程文件 |
| `src/toolchain/testhost/README.md` | NEW | 目录职责(六段) |
| `src/toolchain/workload/desktop/shell/main.rs` | NEW | desktop 薄壳:链 libz42 + 调 z42_run_app |
| `docs/design/testing/embedded-app-run.md` | NEW | 设计原理:嵌入式 app 运行 + 静/动链接 + 4 平台共享路径 |

**只读引用**:`z42-hostrun`(spawn 路,对照)、`desktop/tests/r1_r7.c`(既有 C 嵌入实证,泛化范式)、
`z42.test/src/Runner.z42`(RunModule)、`scripts/test/xtask_test_desktop.z42`(desktop 构建现状)。

## Out of Scope

- **不动 apphost**(framework-dependent 通用部署路保留;测试专走嵌入)。
- **不做 wasm/ios/android 落地**(本 change 只 desktop 参考 + 共享基座;其余平台后续 change 复用)。
- **不做 install/launch/命令通道/结果汇总的完整 harness**(desktop 参考先用最简 args/stdout;
  完整命令通道后续 change)。
- **不填 WorkloadBase 全 5 相位**(desktop 参考先用最短构建路;完整 workload 流水线后续)。

## Open Questions

- `run_app` 便捷入口放 `z42-host`(倾向,少 crate churn)还是独立 `z42-apprun` crate?——倾向 z42-host。
- test-agent 目录 `src/toolchain/testhost/`(倾向)确认。
