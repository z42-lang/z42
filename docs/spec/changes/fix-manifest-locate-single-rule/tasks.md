# tasks: fix-manifest-locate-single-rule

> 类型：**fix**（最小化模式）｜ 创建：2026-09-26
> 出身：[结构审计 2026-09](../../../reference/src/toolchain/z42-toml.md) 的 B-1。

## Why

「工程清单在哪」这个问题有**四份判据**，而权威那份最宽、另三份都更严：

| 位置 | 判据 | 结果 |
|---|---|---|
| `ManifestLocator.FindIn`（权威）| 先裸 `z42.toml`，再唯一一份 `*.z42.toml`，多份 → Ambiguous 并列候选 | — |
| `PathDepPlan._resolveDepToml` | 只 glob `"*.z42.toml"` | **漏裸名** |
| `BuildPaths._handlerFromPath`（`[analyzers]` path）| 只 glob `"*.z42.toml"` | **漏裸名** |
| `builder_publish._pubResolveDepToml` | 硬编码 `<depName>.z42.toml` | **漏裸名 + 要求文件名等于依赖名**，且失败是静默 skip |

后果（用户级）：**`z42 new` 造出来的工程当不了 path 依赖。**`builder_new.z42:31` 写的是
**裸 `z42.toml`**，而 `"*.z42.toml"` 要求文本含字面 `.z42.toml` 后缀 —— `z42.toml` 里 `z42`
前没有点 ⇒ 不匹配。报错文本「期望恰 1 份 `*.z42.toml`，实得 0」离真正的原因很远。

⭐ **它藏住的原因**：全部 e2e fixture 都写 `<name>.z42.toml`，恰好落在两份判据的交集里。

## What Changes

三处全部委托 `ManifestLocator.FindIn`，并把诊断从「期望恰 1 份 *.z42.toml」改成说得出真正
原因（没有清单 / 有多份并列出候选 / 那是个 workspace）。只接受 `Kind == Project` ——
`FindIn` 也认 `z42.workspace.toml`，但闭包算法要的是单包清单，把 workspace 当 path 依赖
是另一种错，得说清楚而不是静默当成「0 份」。

## Scope（允许改动的文件）

- `src/libraries/z42.project/src/PathDepPlan.z42`
- `src/compiler/z42c.driver/src/BuildPaths.z42`
- `src/toolchain/builder/core/builder_publish.z42`（+ `using Z42.Build.Project;`）
- `scripts/build/xtask_compiler_e2e.z42`（门禁：把 path 依赖 e2e 的一层改用裸 `z42.toml`）
- `docs/reference/src/toolchain/z42-toml.md`、`docs/reference/src/toolchain/compile-time-extensions.md`

## Tasks

- [x] 三处委托 `ManifestLocator.FindIn` + 诊断说明真正原因
- [x] 门禁：path 依赖 e2e 的 **baz 层改用裸 `z42.toml`**（foo/bar 仍用 `<name>.z42.toml`）
      ⇒ 两种拼写在同一次 e2e 里都被走到；修前这条 e2e 必红
- [x] 参考手册 4 处「恰一份 `*.z42.toml`」断言改为「恰一份工程清单」+ 说明判据与
      `z42c build <dir>` 一致
- [ ] GREEN：`xtask test e2e`（含 path 依赖 e2e）本地过；CI 全矩阵绿

## 不做（Out of Scope）

- **不改 `builder_new.z42` 写的文件名**。裸 `z42.toml` 是 `ManifestLocator` 的**首选**形态，
  是正确的那一侧；该改的是另外三份判据。
- **不动 `WS005`（workspace 成员目录两份清单）那条**。它是 workspace 成员发现的判据，
  与 path 依赖正交，且当前零发射点（见错误码表）——单独登记。

## 验证

- 阴性对照：把 baz 的清单改回 `pathdep.baz.z42.toml` ⇒ e2e 恢复绿（证明这一格确实在测新判据）；
  或撤回三处委托 ⇒ e2e 必红。
- 无格式 bump、无指纹 bump：只放宽「清单文件叫什么」的接受面，不改任何产物字节。
