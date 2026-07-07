# Tasks: ci-hardening

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：chore/ci（直接实施）

**变更说明：** 落地 `docs/xtask_review.md` 第三节 CI 的**低风险高价值子集**——job 超时护栏、
`cargo install` 换预编译二进制、bench-pr 触发 glob 修正 + 缓存统一。
**原因：** 全 CI 无 `timeout-minutes`（挂死按 6h 计费）；`cargo install` 每次源码编译（分钟级）；
bench-pr 的 `scripts/xtask*.z42` glob 在子目录化后漏掉大部分 xtask 源 → 改这些源的 PR 不触发性能门。
**文档影响：** 无对外行为变更（CI 基建）。本地不可跑 CI，验证 = `actionlint` 静态校验 + 人工审阅。

**子系统锁**：CI/.github 不上锁（同 docs）。

## 任务
- [x] 1.1 `timeout-minutes` 加到全部 workflow 的全部 job（ci.yml 19 + release.yml 3 + bench-pr 1
      + bench-update 1 + deploy-book 2）——原本全 CI 零 timeout，挂死按 6h 计费
- [x] 1.2 `cargo install {cargo-ndk,wasm-pack,wasm-tools}` → `taiki-e/install-action`（6 处：
      ci.yml 4 + release.yml 2；预编译二进制秒级，worst-case 回退 cargo install 故无风险）
- [x] 1.3 bench-pr.yml：`scripts/xtask*.z42` → `scripts/**/*.z42`（子目录化后改 scripts/test|build|
      package 的 PR 也触发性能门）；手写 `actions/cache@v4` → Swatinem `shared-key: host-v2`
- [x] 1.4 `actionlint` 全 workflow 通过（仅剩 pre-existing shellcheck info SC2012/SC2015，非本次引入）
- [x] 1.5 归档

## Out of Scope（风险/依赖，留后续独立评估）
- 删 ci.yml `bench-e2e`（与 bench-pr 重叠）——**可能是 required status check**，删了会卡住所有 PR；
  需先确认 branch protection 配置（User 侧）。
- §3.1 vm-jit/stdlib-jit 8 shard 改消费 toolchain 工件——JIT golden 的 .zbc fixture 在工件路径下
  是否齐备需 CI 侧验证（本地不可验），错了会破 JIT 门。
- §3.4 `test dist`/`test packages` 进 CI；§3.5 归档 shell 收敛为 `package archive` 命令；
  android-emu 重试/gating（需 User 定 gating 语义）。

## 备注
（实施中记录）
