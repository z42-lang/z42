# Design: 让配置文件层真正到达已发布的 app

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)

## Architecture

```
                        ┌──────────────── 谁装配置 ────────────────┐
z42vm main()            │ CLI + env + Z42_CONFIG + Z42_APP_CONFIG │ 致命诊断 + --strict-config
                        │  → init_runtime_config(cfg)             │
                        └─────────────────────────────────────────┘
                        ┌─────────────────────────────────────────┐
其它一切入口（嵌入方）    │ runtime_config() 懒初始化 → from_env()   │ ← P0：此前只有 env，
  z42_host_run_app      │   env + Z42_CONFIG + Z42_APP_CONFIG     │    现在补上两个文件层
  （desktop 自包含 /    │  → warn，绝不 exit                       │
    wasm / iOS /        └─────────────────────────────────────────┘
    Android / testhost）

spawn apphost:  exec_app → z42vm <app.zpkg>
                  + Z42_LIBS（既有）
                  + Z42_APP_CONFIG=<app 同目录同 stem>.runtimeconfig.toml  ← P1

z42 publish:    dist/<name>.zpkg          → 布局内
                dist/<name>.runtimeconfig.toml → 跟着走（自包含布局随 zpkg 改名）  ← P2
```

## Decisions

### Decision 1：`from_env()` 补文件层，而不是新增一个 `from_env_with_files()`

**选定**：直接让 `from_env()` 加载 `Z42_CONFIG` / `Z42_APP_CONFIG`。

**理由**：`from_env` 的语义是"从进程环境构建配置"，而**环境正是文件路径的来源**——
`Z42_CONFIG` 就是一个环境变量。今天它只读一半环境，是实现遗漏而非设计选择：机制页
和旋钮表都把这两层写成链的一部分，只有 `z42vm` 的 `main()` 兑现了。新加一个
`_with_files` 变体等于把"完整的链"变成需要各入口显式选择加入的东西——每个新嵌入方
都会再漏一次，而漏的表现是**静默无效**。

**非破坏性**：`from_getter`（测试与可注入路径）保持 `resolve(_, None)` 不变；只有
`from_env` 这一个真实环境入口改变。`z42vm` 的 `main()` 不走 `from_env`（它自己装配
CLI 层后 `init_runtime_config`），行为逐字节不变。

**代价**：第一次 `runtime_config()` 会做最多两次文件读。它发生在 boot 期一次
（`OnceLock`），与 `main()` 早就在做的事相同。

### Decision 2：库入口只 warn，绝不退出

`main()` 对坏配置是 `exit(2)`——它是进程的主人。`from_env()` 是**库路径**，可能跑在
宿主进程里（iOS app、Android JNI、wasm、testhost）；在那里 `exit` 掉宿主是不可接受的。

所以文件层的错误（非法 TOML / `.json` / `[runtime]` 不是表）在这条路径上降级为
一行 `eprintln!` + 该层视为不存在。**这与既有分层策略一致**：env/文件层的问题本来
就是 warn（`complete-runtime-settings` Decision 3）；差别只在 `main()` 额外提供了
`--strict-config` 把它升级——库入口没有那个开关，也不该有。

### Decision 3：apphost 只在调用方没设 `Z42_APP_CONFIG` 时注入

与 launcher 同款语义：显式设置优先。apphost 是**发现**侧车（按"与 zpkg 同目录同
stem"的约定），不是**规定**侧车。

**为什么 apphost 不自己解析侧车**：它是一个零依赖的原生 stub，职责是"找到 z42vm +
把 app 交给它"。解析 TOML、判定旋钮可用性、产生诊断都属于 VM。apphost 只传一个路径。

**为什么不复用 launcher 的发现逻辑**：apphost **刻意不经 launcher**
（`simplify-apphost-direct-run`：部署一个 app 只需 apphost + app.zpkg + 运行时，
不需要 `launcher.zpkg`）。这里重复的是一行 `<stem>.runtimeconfig.toml` 的路径拼接，
不是逻辑；把 launcher 拉进 apphost 的依赖只为省这一行是坏交易。

### Decision 4：publish 拷侧车按"跟着 zpkg 走 + 跟着改名"

三处拷贝各自的目标名不同：

| 布局 | zpkg 落点 | 侧车落点 |
|---|---|---|
| payload（`[platform.desktop].payload`）| `root/<payloadRel>` | 同目录，stem 取自 `payloadRel` |
| 无 payload | 留在 `dist/`（不拷）| 不动（已在 dist/ 里）|
| 自包含（embed）| `appDir/app.zpkg`（**改名**）| `appDir/app.runtimeconfig.toml` |

自包含布局把 zpkg 改名成 `app.zpkg`，侧车必须跟着改成 `app.runtimeconfig.toml`——
"同目录同 stem"是发现约定，拷过去还叫原名等于没拷。

**侧车不存在 → 什么都不做**：多数工程没有 `[profile.*]` 运行时旋钮，因而没有侧车
（`complete-runtime-settings` 的"无旋钮不产文件"）。publish 不该为此报错或产空文件。

## Implementation Notes

- `from_env` 里两个文件层的加载复用 `load_runtime_toml` / `load_app_config`（已存在），
  错误分支从 `?` 改为 `unwrap_or_else(|e| { eprintln!(..); None })`。
- `exec_app` 的发现要用 `app_zpkg.with_extension("runtimeconfig.toml")`——注意
  `with_extension` 会替换**最后一个**扩展名，`app.zpkg` → `app.runtimeconfig.toml`，正确。
- publish 的三处拷贝抽一个 `_pubCopySidecar(zpkgSrc, dstZpkg)` helper（源侧车 =
  `zpkgSrc` 同目录同 stem；目标 = `dstZpkg` 同目录同 stem），三处各调一次。
- `builder_publish.z42` 现 967 行、在 line-limit 棘轮基线上——**新增行会让门禁变红**。
  helper 放新文件 `builder_publish_sidecar.z42`，三处调用点各 1 行 = +3 行 →
  仍会超基线。**故 helper 与三个调用点合计必须净不增**：把 helper 与它替换掉的
  `File.Copy(zpkgSrc, ...)` 一起搬进新文件（`_pubStageApp(zpkgSrc, dst)` = 拷 zpkg +
  拷侧车），三处各由 1 行 `File.Copy` 变 1 行调用 → 净 0。

## Testing Strategy

| 层 | 测试 |
|---|---|
| `from_env` 文件层 | `Z42_CONFIG` 指向的 `[runtime]` 值在 `runtime_config()` 上生效；`Z42_APP_CONFIG` 同理；两层叠加用户赢；坏 TOML → warn 且**不 panic/不退出**、该层视为不存在 |
| 非破坏 | `from_getter(fake_env)` 逐字段不变（文件层只在 `from_env` 上）；`z42vm --show-config` 输出不变 |
| apphost | 侧车存在 → 命令带 `Z42_APP_CONFIG`；不存在 → 不带；调用方已设 → 不覆盖；`app.zpkg` → `app.runtimeconfig.toml`（stem 派生正确）|
| publish | payload 布局侧车落在 zpkg 旁；自包含布局侧车改名为 `app.runtimeconfig.toml`；无侧车时不报错 |
| e2e | `z42 publish` 一个带 `[profile.release] gc-trace = true` 的工程 → 跑产出的二进制 → stderr 出现 GC trace 行（**端到端证明 profile 到达了已发布 app**）|

## Out of Scope
- 嵌入方的 CLI 层（`--set` 是 z42vm 二进制的表面）。
- iOS/Android/wasm 各自的"侧车怎么随包分发"（打包问题，经 Decision 1 自动受益于环境变量）。
- `runtimeconfig.template.toml` 合并、8 个 `ENV_ONLY` 旋钮收编（均为独立项）。
