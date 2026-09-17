# Proposal: workload 命令分发 B1+B2

> 状态：DRAFT（前置：无硬性依赖，__load_zpkg VM builtin 非必须）
> 里程碑：0.3.14

## Why

当前 launcher 内所有命令（build / run / export / publish / workload install…）全部
baked 进同一个 `launcher.zpkg`。每加一个平台命令（如 `z42 publish android`）都要
重编并重新分发 launcher，与平台 SDK 的发布节奏强耦合。

B2 定义"命令 workload 包格式"——workload 不只包含运行时（iOS/Android/WASM runtime），
还可携带命令 zpkg + `.cmd.toml` 元数据。
B1 实现"命令发现"——launcher 启动时扫描已安装 workload 的命令目录，动态注册到
Std.Cli 树（spawn-leaf 节点），`z42 -h` 自动列出，无需重编 launcher。

## What Changes

1. **`.cmd.toml` 格式**：定义命令 workload 清单（verb、description、zpkg 路径）。
2. **Std.Cli 扩展**：`SubcommandRouter.AddSpawn(verb, desc, zpkgPath)` ——
   注册一个"spawn 式叶子"：被触发时 `z42vm <zpkgPath> -- <余下 argv>` 透传。
3. **Launcher 发现**：启动时扫描
   `$Z42_HOME/runtimes/<ver>/workloads/<wl>/commands/*.cmd.toml`，
   读取 verb/desc/zpkg，调 `AddSpawn` 注册到对应子树。
4. **命令 workload 包格式**：定义 `workload.toml`（现有）需包含 `[commands]` 段，
   `z42 workload install` 解包时额外解到 commands/ 目录。
5. **验证**：安装一个示例命令 workload（如 `z42 greet`），`z42 -h` 可见，
   `z42 greet` 跑出预期输出。

## 前置依赖

| 依赖 | 状态 | 说明 |
|------|------|------|
| `z42.io.Process`（subprocess spawning）| ✅ 已有 | launcher 已用于 run/z42vm |
| Std.Cli `SubcommandRouter` | ✅ 已有 | 需扩 `AddSpawn` 方法 |
| `z42 workload install`（本地 install）| ✅ 已有（impl-workload-install 2026-06-17）| B2 命令包只需解包到 commands/ |
| `runtime-dynamic-load-call` VM builtin | ❌ 不需要 | spawn 式分发不需要 in-process 动态加载 |

## Scope

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.cli/src/SubcommandRouter.z42` | MODIFY | 新增 `AddSpawn(verb, desc, zpkgPath)` |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | 启动时调用命令发现逻辑 |
| `src/toolchain/launcher/core/launcher_workload_commands.z42` | NEW | `WorkloadCommandDiscovery.Discover()` — 扫目录 + 注册 |
| `src/toolchain/launcher/core/launcher.z42` | MODIFY | `_spawnCommand(zpkgPath, argv)` 辅助（复用现有 spawn 逻辑）|
| `src/toolchain/workload/desktop/README.md` | MODIFY | 文档：命令 workload 格式说明 |
| `docs/design/toolchain/launcher-command-dispatch.md` | MODIFY | 将 B1/B2 从 Deferred 移出，补实施决策 |
| `docs/internals/src/toolchain/workload-distribution.md` | MODIFY | 补 commands/ 目录结构说明 |
| `examples/workloads/greet/` | NEW | 示例命令 workload（`z42 greet` 命令，用于验证） |

**只读引用**：

- `src/libraries/z42.cli/src/ArgParser.z42` — 理解 Std.Cli 现有结构
- `src/toolchain/launcher/core/launcher_workload.z42` — 现有 workload install 逻辑

## 命令 workload 包格式（B2）

```
workloads/<wl>/
├── runtime/          ← 现有：runtime zpkg（z42vm 等）
├── commands/         ← 新增：命令条目目录
│   ├── greet.cmd.toml
│   └── greet.zpkg
└── workload.toml     ← 扩展 [commands] 段
```

`greet.cmd.toml` 示例：
```toml
verb = "greet"
description = "Say hello from the greet workload"
zpkg = "greet.zpkg"
min_runtime = "0.3.14"
```

## Out of Scope

- **网络下载 workload**（按需 auto-download）：依赖 GitHub Releases 格式定稿，延后
- **`workloads` 项目 manifest 段**（build 时自动下载）：延后
- **命令版本冲突 / 多版本共存**：延后
- **B4 平台一致性测试生成**、**B5 完整 export/publish 生命周期**：延后
- **in-process 命令加载**（`__load_zpkg`）：延后，现阶段 subprocess 够用

## Open Questions

- [ ] `AddSpawn` 时 help 文本从哪里来？
      → 建议：`.cmd.toml` 里的 `description` 字段；支持 `z42 <cmd> --help` 透传给 zpkg
- [ ] 发现顺序：多个 workload 提供同 verb 时谁赢？
      → 建议：按 workload 名字母序 first-wins（与 common-pitfalls.md §1 对齐，显式 sort）
- [ ] 示例 greet workload 放 examples/ 还是 src/toolchain/workload/greet/？
      → 建议：examples/workloads/greet/ 作为用户面示例，工具链测试另建
