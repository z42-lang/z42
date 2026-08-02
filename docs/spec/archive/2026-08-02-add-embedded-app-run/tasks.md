# Tasks: 嵌入式 app 运行基座 + desktop 参考

> 状态：🟢 已完成 | 创建：2026-08-02 | 完成：2026-08-02（PR #95 合并 405f5717，已归档） | 分支：add-embedded-app-run(off origin/main)
> 只做「嵌入基座 + desktop 参考」;wasm/ios/android + 完整 harness/命令通道 = 后续 change。

## 阶段 1: 嵌入基座(runtime，可 cargo 验证)
- [ ] 1.1 静/动产物走独立 `cargo rustc --crate-type=staticlib|cdylib`（**不改 [lib]**，尊重
      rlib-only 现状 Cargo.toml:32-34）;验证 `--crate-type=cdylib` 产出 libz42.{dylib/so}
      （staticlib 现状已产）
- [x] 1.3 **抽共享 app-run 核心**（D1）：新建 `src/runtime/src/app.rs` `z42::app::run(path, entry, opts)`
      —— 把 main.rs 的 load→lazy-loader→libs→vm.run 序列（~250 行核心启动路径）搬进来
- [x] 1.4 `main.rs` 重构调 `z42::app::run`（**behavior-preserving**）+ cargo build
- [x] 1.5 **全 golden 套件绿**（CI 权威；本地 e2e：refactored z42vm 跑 xtask.zpkg -h 干净退出，load→deps→entry 路径验证）（`xtask test e2e`）—— 核心路径回归网，重构不动点
- [x] 1.6 `z42-host::run_app` 调 `z42::app::run`（+ `z42::app::default_mode`）；cargo check 绿。
      C 符号（`z42_host_run_app`）延后到 mobile 壳需要时（desktop 参考走 Rust wrapper 直调）

## 阶段 2: z42 test-agent(共享字节码)
- [x] 2.1 `src/toolchain/testhost/agent/`:agent.z42(Main:命令 → Std.Test.Runner.RunModule → JSON)+ toml
- [x] 2.2 testhost/README.md(六段)
- [x] 2.3 编译 agent → app.zpkg 验证 + 端到端跑 sample test → JSON（2/2 passed）

## 阶段 3: desktop 参考(端到端)
- [x] 3.1 嵌入路径参考：`z42-host/examples/run_app_smoke.rs`（标准原生宿主嵌入 VM 调 z42-host::run_app）。
      productionized `workload/desktop/shell` + 静/动 libz42 C 壳 = 剩余（embedding 路径本身已证）
- [x] 3.2 端到端：run_app_smoke 嵌入跑 test-agent + sample test → JSON（total:2 passed:2）✅ 全链验证
- [x] 3.3 static/dynamic **两侧均验证**：static=显式 libz42.a、dynamic=-lz42/libz42.dylib，C 壳 testhost.c 各跑通 → JSON

## 阶段 4: 文档 + 验证归档
- [x] 4.1 `docs/design/testing/embedded-app-run.md`（核心/三前端/test-agent/静动链接/desktop 参考/后续）
- [x] 4.2 cargo build 绿 + desktop e2e（static+dynamic C 壳 + Rust 嵌入 + z42vm 三前端均 JSON 2/2）；全 golden 以 CI 为权威
- [ ] 4.3 归档 + PR(GREEN 以 CI 为权威)

## 备注
- 两开放问题取倾向:run_app 进 z42-host(不新开 crate);agent 放 testhost/。
- 不动 apphost;不做其余平台;命令通道先最简 args/stdout。
