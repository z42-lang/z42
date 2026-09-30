# Tasks: driver 改用 ManifestKnobs / CompilerDomain（unify-driver-knobs）

> 状态：🟢 已完成 | 创建：2026-09-30 | 类型：refactor（最小化模式；BuildSession 合并第二步的第一刀）

**变更说明**：add-build-session（#970）为 z42b 新建了 `ManifestKnobs`（opt / syntax / lints / pack / strip 决议）与
`CompilerDomain`（编译器域解析域），语义逐项搬自 driver，两份并存、注释要求「改一份须同步另一份」，重复窗口按
bootstrap-seed 纪律等一个 nightly。含 #970 的 nightly（main @ 9ddc3e6）已发布 ⇒ driver 可以改用它们。

**原因**：消掉重复实现（同一名字表、同一探测序各写一份，迟早漂移）。

**文档影响**：pipeline README（ManifestKnobs / BuildSession 两行）；知识从 driver 注释迁到 `ManifestKnobs.z42` 头注。

## 任务
- [x] 1.1 `Main._build`：`[optimize]` / `[syntax]` 决议与 `[lints]` → `LintConfig` 改调 `ManifestKnobs.Resolve(pm, tomlPath, …, forcePacked=false)`；
      错误行前缀 `z42c: ` 不变。pack 仍用 driver 自己的角色规则（flat workspace / 闭包子建）且先行拦截 ⇒ Resolve 的 pack 报错不触发
- [x] 1.2 删 driver `_compilerDomainDirs` / `_clContains`，`Main` / `ExeDeps` 改调 `CompilerDomain.Dirs()`；探测序的来龙去脉注释迁到 pipeline
- [x] 1.3 `ManifestKnobs.z42` 头注吸收 driver 原注释里的「为什么」（`[optimize]` 曾被静默忽略的字节对账、`Has` vs `IsEnabled`、按工程决议）
- [x] 2.1 手工验：未知优化名 / 语法特性名文案逐字不变；`xtask_test_incremental` 的子串断言仍成立
- [x] 2.2 `xtask test` 全绿

## 备注
- 行为差异只有一处：`[optimize]` 与 `[syntax]` 同时写错时，此前只报前者即退出，现在两类未知名一并报出（信息只多不少）。
- 只用 nightly 里已有的 `ManifestKnobs.Resolve` / `CompilerDomain.Dirs()` 签名，未新增跨成员符号。
- 余下的第二步（增量 / indexed / 缓存 / path 闭包 / workspace 角色迁入 BuildSession、`Z42cCompiler` 旧路径收掉）另起。
