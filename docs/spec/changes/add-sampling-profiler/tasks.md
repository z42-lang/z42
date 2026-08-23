# Tasks: safepoint 采样 profiler

> 状态：🟢 实现完成（cargo build/test 全绿；e2e/完整 GREEN 以 CI 两代自举为权威——本地格式-bump 墙 +
> 环境 hung-seed，同 P1b/P1c 做法）| 创建：2026-08-24

## 进度概览
- [x] 阶段 1: 采样基建（Sampler + safepoint hook + config）
- [x] 阶段 2: 输出 + xtask 火焰图 + perfetto trace
- [x] 阶段 3: 测试（cargo）+ 文档（book 上浮）；e2e + 完整 GREEN 交 CI

## 阶段 1: 采样基建
- [x] 1.1 `gc/sampler.rs`：`Sampler`（后台定时线程 + sample_pending flag + folded 累加器 + perfetto 帧树 intern +
      样本时间线 + maybe_sample + flush_folded + flush_trace）
- [x] 1.2 `gc/mod.rs`：`pub mod sampler;` + `pub use sampler::Sampler;`
- [x] 1.3 `config.rs`：`Z42_SAMPLE_HZ` / `Z42_SAMPLE_OUT` / `Z42_TRACE_OUT` 进 KNOWN_KNOBS + RuntimeConfig（+parse + 测试）
- [x] 1.4 `vm_context.rs`：VmCore 加 `sampler: Sampler`；new_internal 按 Z42_SAMPLE_HZ start(hz, trace_out.is_some())/disabled
- [x] 1.5 `safepoint.rs`：`check_safepoint_slow` Idle 末 gated 采样 hook

## 阶段 2: 输出 + xtask
- [x] 2.1 `app.rs`：run() 结束 flush folded → Z42_SAMPLE_OUT（+ 设 Z42_TRACE_OUT 时 flush_trace chrome JSON）+ stderr 提示
- [x] 2.2 `xtask_profile.z42` `_profileCpu`：z42-level 火焰图（inferno / .folded）+ perfetto trace 产物（镜像 --heap）

## 阶段 3: 测试 + 文档 + 验证
- [x] 3.1 `gc/sampler_tests.rs`：累加 + folded 格式 + 空栈 + flush 降序 + perfetto JSON（P 事件 + stackFrames 树）单测（7 个，全绿）
- [ ] 3.2 端到端（需 0.42 seed）：**本地 hung-seed 挡，交 CI**——热循环脚本 Z42_SAMPLE_HZ(+TRACE_OUT) 跑 → .folded + trace 合法
- [x] 3.3 `cargo build` + `cargo test --lib`（960 passed / 0 failed）
- [ ] 3.4 `xtask test`（完整 GREEN；格式-bump 期 CI 两代自举权威）——交 CI
- [x] 3.5 **知识上浮**：新建 `docs/book/src/runtime/diagnostics.md` + 挂 SUMMARY
- [x] 3.6 `docs/design/runtime/diagnostics.md` §7/§8 标注采样 + perfetto 落地 + 指向 book 新页
- [ ] 3.7 spec scenarios 逐条覆盖 + self-host（CI）

## 备注
- 采样默认关零成本（运行时 flag gate，非 cargo feature；采样点已 throttle）。
- **perfetto trace 本 change 一并做**（User 裁决 B，2026-08-24）：**采样型** chrome trace（复用同一采样，非 span 埋点），
  故不依赖 diagnostics §4.2、不加热路径成本；trace 记录仅在 `Z42_TRACE_OUT` 设时开启（省内存）。
- 无格式 bump；不新增 z42 API。0.42 seed（origin/main 已 #268 bump）：e2e 验证需下载 post-#268 nightly 作 seed。
