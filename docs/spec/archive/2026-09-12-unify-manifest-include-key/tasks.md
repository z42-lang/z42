# Tasks: 清单里「源文件清单」统一叫 include

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`refactor`（最小化模式）

**变更说明：** 同一个概念（哪些源文件参与编译）在清单里此前有**三种**拼法，统一到 `include`。

| 位置 | 改前 | 改后 |
|---|---|---|
| `[sources]` 段 | `include` / `exclude` | 不变 |
| `[tests]` / `[benches]` / `[examples]` 段 | `include` / `exclude` | 不变 |
| `[[test]]` / `[[bench]]` / `[[example]]` | **`sources`** | `include` |
| `[[exe]]` | **`src`** | `include` |

**为什么选 `include` 而不是 `sources`：**
1. 三个段级配置本来就是 `include`，数组形式是唯一的异类；
2. `include` 自带搭档 `exclude` —— 而数组形式此前**根本无法排除文件**，统一后这个洞可以后续补上；
3. `src` **零使用者**（只有解析器支持过），删掉是白赚。

**边界：只改 TOML 键名，不改 z42 侧字段名**（`RunTarget.Sources` / `ExeTarget` 保持）。
xtask 在冷启动时用**种子 stdlib** 编译，重命名导出字段会触发「晚一个 nightly」的自举纪律
（bootstrap-seed 轴 ③）；而 TOML 键是数据不是 API，且受影响的 5 行全在测试夹具里、
种子链不读它们 ⇒ 同一个 commit 改完即可，零自举风险。

**文档影响：** `docs/design/compiler/project.md` 的清单参考（`[[exe]]` / `[[test]]` 示例 + 一条统一说明）。

- [x] 1.1 `ManifestLoader._parseRunTargets` / `_parseExes` 改读 `include`
- [x] 1.2 在仓夹具 5 处改键（`manifest-targets/basic`、`z42b/dev-target-internal`）
- [x] 1.3 `z42.project` 单测同步（`tests_bench_example_targets.z42` 里 `[[test]]` 的内联 toml）
- [x] 1.4 文档同步 + 全仓旧键 grep 清零
- [x] 1.5 GREEN：`xtask test` 全 13 stage 绿
