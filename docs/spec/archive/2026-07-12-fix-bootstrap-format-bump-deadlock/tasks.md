# Tasks: 两代自举根治格式-bump CI 死结

> 状态：🟢 已完成 | 创建：2026-07-09 | 完成：2026-07-12（收尾 by 后续会话，User 授权）
> 占用子系统：toolchain（+ 触碰 CI/rules/docs）
> 类型：fix/feat（CI 结构性）；不碰语言/格式代码,strict-pin 不动
> 落地条目：Deferred `self-hosting-future-single-vm-bootstrap-gap`（已关闭）

## 进度概览
- [x] 阶段 1: 版本差 gate（检测种子 minor vs 当前 writer minor）
- [x] 阶段 2: 两代自举 bash 编排（Gen1/Gen2 用旧 VM，切换新 VM）
- [x] 阶段 3: **CI 验证——由真实格式 bump 达成**（见下 3.1）
- [x] 阶段 4: 文档 + 归档

## 阶段 1: 版本差 gate
- [x] 1.1 ci-bootstrap [1.5]:读种子 `programs/z42c/z42c.driver.zpkg` header minor + grep 源码 `ZpkgWriterZ.Minor` → 比较
- [x] 1.2 相等 → 现有快路径(零改动);不等 → 进两代分支
- [x] 1.3 SDK 缺 `bin/z42vm` → 明确报错兜底

## 阶段 2: 两代自举编排（bash 先行，D4）
- [x] 2.1 Gen1:旧 VM(SDK bin/z42vm) + 旧 z42c → 编当前 z42c 源 + stdlib 源(Z42_LIBS=旧 stdlib)→ gen1 产物
- [x] 2.2 (gen1 minor==旧壳,本地端到端验证) 校验 gen1:minor==旧(外壳)、旧 VM 能加载
- [x] 2.3 Gen2:旧 VM + gen1 z42c → 编当前源(Z42_LIBS=gen1 stdlib)→ gen2 产物
- [x] 2.4 (gen2 minor==当前,本地验证 1900) 校验 gen2:minor==当前(新格式)
- [x] 2.5 gen2 z42c 七包 + stdlib flat dist 落标准 artifact 位;新 VM 接管 [2/5..5/5]

## 阶段 3: 验证
- [x] 3.1 **由真实格式 bump 达成**（比人造 bump 更强）：design Testing Strategy 明定「合入后**下一次
      真实格式 bump 才是终极验证**」，人造 bump 分支仅「为降风险，可先…」的可选步。合入（8318ad7e）后
      main 已连续 **5+ 次真实格式 bump（zpkg 0.25 f66543c8 → 0.26 c9633f52 → … → 0.30 adec87eb 收官）**，
      每次 `build-and-test` / `toolchain-bootstrap` / `package-host`（即走 ci-bootstrap 版本差 gate +
      两代自举的 job）**全部 success**（唯二红 f66543c8/c9633f52 均为正交的 `test-android-emu` 基建
      flake，非 bootstrap）。→ 两代自举经现实反复验证，人造 bump 冗余、不再需要。
- [x] 3.2 非 bump 回归:合入后绝大多数 push（无格式 bump，seedMinor==curMinor）走版本差 gate 的
      **相等快路径（零改动）**，CI 全绿、行为不回归（CI 历史印证）。
- [x] 3.3 本地已用真实 0.24 种子 + 人造 0.25 bump 端到端跑通(gen1→stdlib→gen2→新VM build xtask)

## 阶段 4: 文档 + 归档
- [x] 4.1 self-hosting.md:328 `~~self-hosting-future-single-vm-bootstrap-gap~~` 已关闭 + roadmap:442 索引已更新
- [x] 4.2 version-bumping.md:92-113「bump 与 xtask↔nightly bootstrap 循环」段已标「✅ 死结已根治」
- [x] 4.3 bootstrap-seed.md 格式漂移风险条补两代自举机制（版本差 gate + 旧 VM Gen1/Gen2 + runtime/compile stdlib 分离 + 5+ 真 bump 全绿）
- [x] 4.4 ACTIVE.md 释 toolchain 锁；归档

## 备注
- **本地不可完整验证**(走 CI download 路径)——阶段 3.1 的人造-bump-分支是主要验证手段,
  接受它烧一轮 CI。
- 依赖但不改 support-先行纪律(语法/API 维度);两代只治格式维度。
- 首次落地无鸡蛋:现网 nightly(本次手动修的 0.24)已带 bin/z42vm。
