# Tasks: 嵌入式 app 运行基座 + desktop 参考

> 状态：🟡 进行中 | 创建：2026-08-02 | 分支：add-embedded-app-run(off origin/main)
> 只做「嵌入基座 + desktop 参考」;wasm/ios/android + 完整 harness/命令通道 = 后续 change。

## 阶段 1: 嵌入基座(runtime，可 cargo 验证)
- [ ] 1.1 静/动产物走独立 `cargo rustc --crate-type=staticlib|cdylib`（**不改 [lib]**，尊重
      rlib-only 现状 Cargo.toml:32-34）;验证 `--crate-type=cdylib` 产出 libz42.{dylib/so}
      （staticlib 现状已产）
- [x] 1.3 **抽共享 app-run 核心**（D1）：新建 `src/runtime/src/app.rs` `z42::app::run(path, entry, opts)`
      —— 把 main.rs 的 load→lazy-loader→libs→vm.run 序列（~250 行核心启动路径）搬进来
- [x] 1.4 `main.rs` 重构调 `z42::app::run`（**behavior-preserving**）+ cargo build
- [x] 1.5 **全 golden 套件绿**（CI 权威；本地 e2e：refactored z42vm 跑 xtask.zpkg -h 干净退出，load→deps→entry 路径验证）（`xtask test e2e`）—— 核心路径回归网，重构不动点
- [ ] 1.6 `z42-host::run_app` 调 `z42::app::run`；`z42-abi::z42_run_app` C 包装；cargo build 绿

## 阶段 2: z42 test-agent(共享字节码)
- [ ] 2.1 `src/toolchain/testhost/agent/`:agent.z42(Main:命令 → Std.Test.Runner.RunModule → JSON)+ toml
- [ ] 2.2 testhost/README.md(六段)
- [ ] 2.3 编译 agent → app.zpkg(seed z42c)验证

## 阶段 3: desktop 参考(端到端)
- [ ] 3.1 `workload/desktop/shell/main.rs`:链 libz42(static)+ 调 z42_run_app 跑 agent
- [ ] 3.2 端到端:desktop shell + 嵌入 z42vm 跑一个 test 用例 → 结构化结果(stdout JSON)
- [ ] 3.3 static/dynamic 各验证一次(desktop 可切)

## 阶段 4: 文档 + 验证归档
- [ ] 4.1 `docs/design/testing/embedded-app-run.md`(嵌入 run + 静/动链接 + 4 平台共享路径)
- [ ] 4.2 cargo build + desktop e2e 绿
- [ ] 4.3 归档 + PR(GREEN 以 CI 为权威)

## 备注
- 两开放问题取倾向:run_app 进 z42-host(不新开 crate);agent 放 testhost/。
- 不动 apphost;不做其余平台;命令通道先最简 args/stdout。
