# Proposal: app-config 层跟着 app 走（app-config-follows-the-app）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`vm`（+ `toolchain` 删冗余）→ 完整流程
> 前置：[`sidecar-reaches-published-apps`](../sidecar-reaches-published-apps/)（#446 已合并）
> SoT：`docs/book/src/runtime/runtime-settings.md`

---

## Why

`z42c build` 把工程 `[profile.*]` 烤成 `<app>.runtimeconfig.toml` 放在 zpkg 旁边。
但 **app-config 层只在别人主动指路时才存在**——`Z42_APP_CONFIG` 得由 launcher 或
spawn apphost 设进来。于是：

```
$ ls dist/
demo.runtimeconfig.toml   demo.zpkg          # 侧车就在旁边
$ z42vm dist/demo.zpkg
gc-trace=(default) source=default            # 被无视
```

**受影响的运行形态**（探查确认）：

| 形态 | 现状 |
|---|---|
| `z42 run` / `z42 repl`（launcher）| ✅ launcher 设 `Z42_APP_CONFIG` |
| `z42 publish` 出的 spawn apphost | ✅ apphost 自己发现（#446）|
| **`z42vm <app.zpkg>` 直跑** | ❌ **无视旁边的侧车** |
| **桌面自包含 apphost**（`z42_host_run_app` 进程内）| ❌ 没人设那个环境变量 |
| **wasm / iOS / Android** | ❌ 同上，且那些平台根本没有"用户设环境变量"这回事 |

后三行合起来就是队列里那条「移动/wasm 的侧车分发」——但根因不是"怎么把侧车装进
包里"（zpkg 已经在包里，侧车跟它同目录也就在包里），而是**没人去看它**。

**对照 dotnet**：host 永远读 `<app>.runtimeconfig.json`，不需要任何人"指路"——
那是 app 的属性，不是调用方的选项。z42 现在把它做成了调用方的选项。

---

## What

### A. app-config 层的路径由 **app 文件**推导

`Z42_APP_CONFIG` 已设 → 用它（显式优先，不变）。
未设 → 取 `<app 文件同目录>/<同 stem>.runtimeconfig.toml`，存在则作为 app-config 层。

落在**一处**：`z42vm` 的 `main()`（它有 `cli.file`）与 `z42-host::run_app`
（它有 `file` 参数）各调同一个 `config::sidecar_for(app_file)`。这两条就是**全部**
进入 `z42::app::run` 的路径，覆盖上表所有形态。

### B. 删掉 apphost 里现在冗余的那份发现逻辑

`hostrun.rs::app_config_sidecar`（#446 加的）纯粹是"算出路径再交回给能自己算的那个
东西"。约定归 VM 一处，apphost 回到它的本分：找 z42vm + 把 app 交给它。

**launcher 保留**：它为了 `version` pin 本来就把侧车**读进来**了，路径已在手里，
顺手传下去不是重复发现、是转发一个已知值。

---

## What This Does NOT Do

- **不改优先级链**：app-config 仍是最低的输入层，`Z42_CONFIG`（用户）压过它。
- **不引入包内资源查找**：侧车靠文件系统同目录约定；wasm 的虚拟 FS 已由
  `fs_backend` 抹平，iOS/Android 的 app bundle 里 zpkg 与侧车本就同目录。
- **不做 `runtimeconfig.template.toml` 合并**（独立项）。
- **不动 launcher**。

## 三阶段

| 阶段 | 内容 | 风险 |
|---|---|---|
| **P0** | `config::sidecar_for(app_file)` + `main()` 接入 | 低 |
| **P1** | `z42-host::run_app` 接入（覆盖 embed / wasm / iOS / Android）| 中（碰嵌入入口）|
| **P2** | 删 apphost 的冗余发现 + 其 4 个单测 | 低 |

## Scope
`src/runtime/src/config/source.rs` · `config.rs` · `config_tests.rs` · `main.rs` ·
`crates/z42-host/src/lib.rs` · `workload/desktop/platform/apphost/src/hostrun.rs` ·
`docs/book/src/runtime/runtime-settings.md`

## 未决
无。
