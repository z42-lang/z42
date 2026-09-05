# Proposal: 编译器校验 `[profile.*.runtime]` 的旋钮名

## Why

运行时配置系统（complete-runtime-settings 一线 9 个 PR）已经把「工程 manifest 声明旋钮 →
`z42c build` 烤成侧车 → 目标机 VM 五层链解析」这条路打通。但**旋钮名写错**这件事，
现在只有**运行时**发现得了：

```toml
[profile.release.runtime]
gc-mdoe = "concurrent"      # typo
```

`z42c build` 原样把它烤进 `dist/<app>.runtimeconfig.toml`，构建**全绿**；直到目标机上
每次启动刷一行 `unknown runtime knob \`gc-mdoe\` from [app-config] — ignored`。

三个问题：

1. **发现得太晚**。写 manifest 的人和看见那行警告的人往往不是同一个，中间还隔着一次发布。
2. **说的地方不对**。诊断打在**用户的机器上**，而错在**开发者的 manifest 里**。
3. **说了也白说**。运行时只能"忽略并继续"（见 restructure-profile-sections 的裁决：
   跨机器传播的层不得致命），于是这行警告每次启动都刷、每次都没人修。

而 `--set gc-mdoe=x` 在 CLI 上是**当场致命 + 最近邻建议**的。同一个 typo，写在命令行里立刻
被拦下，写进 manifest 却能一路发布出去——这个不对称没有道理。

## What Changes

- `z42c build` 在**编译前**校验全部 `[profile.<n>.runtime]` 的键名：不在旋钮登记表里的
  → **warning**（带最近邻建议），build 仍成功。
- 旋钮全集**直接问运行中的 VM**（`Std.Runtime.RuntimeConfig.Names()`）——z42c 自己就跑在
  z42vm 上，零新增数据文件、零 SoT 复制。
- 顺带把已有的「`[profile.<n>]` 下直接写键」检查从侧车写出路径挪到同一处：它此前只在
  `isExe` 时跑，**库工程写错了没人说**。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.driver/src/ProfileKnobs.z42` | NEW | `_validateProfileKnobs` + 旋钮全集 / 最近邻 |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `_build` 早期调用校验 |
| `src/compiler/z42c.driver/src/RuntimeConfigSidecar.z42` | MODIFY | 移走 `BadKeys` 检查 |
| `src/compiler/z42c.driver/z42c.driver.z42.toml` | MODIFY | 加 `z42.text` 依赖（Levenshtein） |
| `scripts/build/xtask_compiler_e2e.z42` | MODIFY | `_e2eKnobChecks` 三条断言 |
| `docs/book/src/runtime/runtime-settings.md` | MODIFY | 机制页：诊断分工加"构建期"一层 |
| `src/compiler/z42c.driver/README.md` | MODIFY | 功能索引 + 核心文件 |

**只读引用**：

- `src/runtime/src/config/cli.rs` — `suggest_key` 的阈值（本实现逐字对齐）
- `src/runtime/src/config/resolve.rs` — 文件层的 key 查找语义（只认 `toml_key`）
- `src/runtime/src/corelib/config.rs` — `__cfg_names` 的 `public_key` 语义（元旋钮返回 env 名）
- `src/libraries/z42.project/src/ManifestLoader.z42` — `Knobs` / `BadKeys` 的产出

## Out of Scope

- **不**在构建期判定「已知但本 build 不可用 / 这一层不能设」——见 design.md Decision 2。
- **不**改运行时那一侧的任何诊断（严重度、文案、层级）。
- **不**动 `[profile.<n>.properties]`：应用属性按设计就是 VM 不理解、不校验的。
