# Tasks: add-guid-lazy-version

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03

**变更说明：** 给 `z42.core`（prelude）补 C# corelib 小类型 `Std.Guid` / `Std.Version` /
`Std.Lazy<T>`。为让 `Guid.NewGuid` 取 OS 熵，把 `__crypto_random_bytes` 从 `z42.crypto` 重分类为
core cross-cutting 原语 `Std.Runtime.Entropy`，`SecureRandom` 改委托。
**原因：** corelib 对齐（口令「推进 corelib 对齐」backlog #7）。
**文档影响：** `src/libraries/README.md`（第 2/3 类 + 单一声明点条目重分类熵原语）；
`docs/spec/changes/add-guid-lazy-version/proposal.md` 记录规范冲突裁决。无 lang/ir/vm、无格式 bump。

## 变更分类
`feat`（stdlib）。三类型纯 additive；熵重分类是跨库 native 归属搬迁 + README 规范更新 → 带 proposal 的轻量流程。

## 任务
- [x] 1.1 `z42.core/src/Entropy.z42`：`Std.Runtime.Entropy`（镜像 `Clock`）单一声明 `__crypto_random_bytes`
- [x] 1.2 `z42.crypto/src/SecureRandom.z42`：删 extern，`GetBytes` 委托 `Entropy.GetBytes`
- [x] 2.1 `z42.core/src/Lazy.z42`：`Lazy<T>`（Func 工厂 + 记忆化，记忆逻辑走私有 `_force()` 保 getter 单表达式）
- [x] 2.2 `z42.core/src/Version.z42`：2–4 段构造 / Parse / TryParse / CompareTo / Equals / GetHashCode / ToString
- [x] 2.3 `z42.core/src/Guid.z42`：NewGuid(v4) / Empty / Parse / TryParse / ToString(D|N) / Equals / GetHashCode
- [x] 3.1 测试 `tests/lazy.z42` / `tests/version.z42` / `tests/guid.z42`
- [x] 4.1 `src/libraries/README.md` 熵原语重分类同步
- [x] 5.1 GREEN：`xtask test` 全 stage ✅（e2e/cross-zpkg/multi-exe/stdlib/manifest/examples/compiler 23 全过 +
      **自举不动点 3/3 gen1==gen2**）+ z42.core 49 file 全过 / z42.crypto 28 file 全过（SecureRandom 委托后仍绿）
- [x] 5.2 归档 changes→archive/2026-09-03-add-guid-lazy-version + 本 tasks 转 🟢，随 PR 一起提交

## 实现期发现（已回填 proposal Deferred）
- **`Guid?`（用户 struct nullable）GREEN 验证可用**——首次在 stdlib 使用。
- **`default(Guid)` 产 Null**（带引用字段 struct 的 z42 default 限制）→ 文档化「用 `Guid.Empty()`」，
  test 不测 default。改纯值（两 i64）表示以修 default 时撞另一 runtime bug（非-`[Record]` 全原始字段
  struct 的 `StructCopy ... got Null`），故保留 byte[] 表示。两个 backlog 根治候选见 proposal。
- **workspace 构建的新跨包 API**（crypto→core.Entropy）在 stale overlay 上需两遍（pass1 建 core、pass2
  crypto 解析到 Entropy）；干净构建（CI）拓扑序一遍即可。

## 备注 / 待 GREEN 验证的风险点
- **`Guid?`（用户 struct 的 nullable）**：stdlib 首次使用，GREEN 验证。若编译不过 → 退回 `Guid.TryParse`
  为 Deferred（只留 `Parse`），并记入 memory。
- **命名属性 getter body**（Lazy/Version 的 `{ get { ... } }`）：`Version` 的属性 getter 是单 `return`；
  `Lazy.Value` 的记忆化下沉到 `_force()` 方法，getter 仍单表达式——规避 StringBuilder 遇到的「命名属性
  只支持计算 getter」限制。
- 熵重分类**无格式 / 种子影响**：builtin 早注册，仅搬 z42 extern 声明。
