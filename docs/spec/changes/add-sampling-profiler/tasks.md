# Tasks: safepoint 采样 profiler

> 状态：🟡 进行中 | 创建：2026-08-24

## 进度概览
- [ ] 阶段 1: 采样基建（Sampler + safepoint hook + config）
- [ ] 阶段 2: 输出 + xtask 火焰图
- [ ] 阶段 3: 测试 + 文档（book 上浮）+ 验证

## 阶段 1: 采样基建
- [ ] 1.1 `gc/sampler.rs`：`Sampler`（后台定时线程 + sample_pending flag + folded 累加器 + maybe_sample + flush_to）
- [ ] 1.2 `gc/mod.rs`：`pub mod sampler;`
- [ ] 1.3 `config.rs`：`Z42_SAMPLE_HZ` / `Z42_SAMPLE_OUT` 进 KNOWN_KNOBS + RuntimeConfig
- [ ] 1.4 `vm_context.rs`：VmCore 加 `sampler: Sampler`；new_internal 按 Z42_SAMPLE_HZ start/disabled
- [ ] 1.5 `safepoint.rs`：`check_safepoint_slow` Idle 末 gated 采样 hook

## 阶段 2: 输出 + xtask
- [ ] 2.1 `app.rs`：run() 结束 flush folded → Z42_SAMPLE_OUT + stderr 提示
- [ ] 2.2 `xtask_profile.z42` `_profileCpu`：z42-level 火焰图（inferno / .folded 产物，镜像 --heap）

## 阶段 3: 测试 + 文档 + 验证
- [ ] 3.1 `gc/sampler_tests.rs`：累加 + folded 格式 + 空栈 + flush 降序单测
- [ ] 3.2 端到端（0.42 seed 就绪）：热循环脚本 Z42_SAMPLE_HZ 跑 → .folded 热函数键 count 最高
- [ ] 3.3 `cargo build` + `cargo test --lib`
- [ ] 3.4 `xtask test`（完整 GREEN；格式-bump 期 CI 两代自举权威）
- [ ] 3.5 **知识上浮**：新建 `docs/book/src/runtime/diagnostics.md`（counter/park/contention/采样统一机制页）+ 挂 SUMMARY
- [ ] 3.6 `docs/design/runtime/diagnostics.md` §7 标注采样落地 + 指向 book 新页
- [ ] 3.7 spec scenarios 逐条覆盖 + self-host（CI）

## 备注
- 采样默认关零成本（运行时 flag gate，非 cargo feature；采样点已 throttle）。
- **perfetto trace 延后**（Deferred，须 User 确认 scoping）。
- 无格式 bump；不新增 z42 API。0.42 seed（origin/main 已 #268 bump）：e2e 验证需下载 post-#268 nightly 作 seed。
