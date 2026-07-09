# Tasks: 两代自举根治格式-bump CI 死结

> 状态：🔴 DRAFT（待 User 阶段 6.5 确认后实施）| 创建：2026-07-09
> 占用子系统：toolchain（+ 触碰 CI/rules/docs；实施时登记 toolchain 锁）
> 类型：fix/feat（CI 结构性）；不碰语言/格式代码,strict-pin 不动
> 落地条目：Deferred `self-hosting-future-single-vm-bootstrap-gap`

## 进度概览
- [ ] 阶段 1: 版本差 gate（检测种子 minor vs 当前 writer minor）
- [ ] 阶段 2: 两代自举 bash 编排（Gen1/Gen2 用旧 VM，切换新 VM）
- [ ] 阶段 3: CI 验证（人造 bump 分支跑通闭环）+ 非 bump 回归
- [ ] 阶段 4: 文档（关闭 Deferred / version-bumping 删手动告警 / bootstrap-seed 补机制）+ 归档

## 阶段 1: 版本差 gate
- [ ] 1.1 ci-bootstrap [1.5]:读种子 `programs/z42c/z42c.driver.zpkg` header minor + grep 源码 `ZpkgWriterZ.Minor` → 比较
- [ ] 1.2 相等 → 现有快路径(零改动);不等 → 进两代分支
- [ ] 1.3 SDK 缺 `bin/z42vm` → 明确报错兜底

## 阶段 2: 两代自举编排（bash 先行，D4）
- [ ] 2.1 Gen1:旧 VM(SDK bin/z42vm) + 旧 z42c → 编当前 z42c 源 + stdlib 源(Z42_LIBS=旧 stdlib)→ gen1 产物
- [ ] 2.2 校验 gen1:minor==旧(外壳)、旧 VM 能加载
- [ ] 2.3 Gen2:旧 VM + gen1 z42c → 编当前源(Z42_LIBS=gen1 stdlib)→ gen2 产物
- [ ] 2.4 校验 gen2:minor==当前(新格式)
- [ ] 2.5 gen2 z42c 七包 + stdlib flat dist 落标准 artifact 位;新 VM 接管 [2/5..5/5]

## 阶段 3: 验证
- [ ] 3.1 人造 bump 分支(临时 writer minor +1)推 CI → 观察两代自举自动过 + publish-nightly 发出新种子 → 闭环确认 → 撤人造 bump
- [ ] 3.2 非 bump 回归:版本相等走快路径,CI 全绿不回归
- [ ] 3.3（若抽 xtask bootstrap-twogen）本地 mock 旧种子干跑编排断言 gen2 minor

## 阶段 4: 文档 + 归档
- [ ] 4.1 self-hosting.md 关闭 `self-hosting-future-single-vm-bootstrap-gap`；roadmap 索引
- [ ] 4.2 version-bumping.md「bump 与 nightly bootstrap 循环」段:死结已根治,删/改手动恢复告警
- [ ] 4.3 bootstrap-seed.md:补两代自举机制(格式维度自愈)说明
- [ ] 4.4 ACTIVE.md 释 toolchain 锁；归档

## 备注
- **本地不可完整验证**(走 CI download 路径)——阶段 3.1 的人造-bump-分支是主要验证手段,
  接受它烧一轮 CI。
- 依赖但不改 support-先行纪律(语法/API 维度);两代只治格式维度。
- 首次落地无鸡蛋:现网 nightly(本次手动修的 0.24)已带 bin/z42vm。
