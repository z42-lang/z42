# Proposal: stdlib API 迁移到 `params`（`String.Concat` / `String.Format`）

## Why

`params` 的「支持」在 `add-params-varargs`（2026-07-01）落地，动机 API `Path.Join` /
`String.Join` 已在 `stabilize-dispatch-keys`（方案 A，2026-07-15 合并 main，zbc 1.27 /
zpkg 0.32）改用 `params`。该变更归档的**阶段 9**（`archive/2026-07-01-add-params-varargs/tasks.md`）
列出剩余候选 API，本变更收尾其中两项：

- `String.Concat(a,b)` + `Concat(a,b,c)` → `params string[]`
- `String.Format(fmt, arg0)` + `Format(fmt, arg0, arg1)` → `params object[]`

阶段 9 的全库二次扫描（本变更复核）确认：除已迁移的 Join 家族外，**仅剩这两组**「同名限-arity
重载 / 编号实参 arg0/arg1」模式。迁移后该 Deferred 项清零。

## 为什么现在可以做（自举约束核实）

阶段 9 的两条硬约束是「z42c 自消费的 API（`Path.Join`）必须晚一代」+「拓扑：z42c 调用的 API 迁移
破不动点」。本变更迁移的 `String.Concat` / `String.Format`：

- **z42c / xtask 均不调用**（grep `src/compiler` `src/toolchain` 零命中——`Diagnostic.Format()`
  是无参实例方法，非 `String.Format`）→ **不破自举不动点**，无需晚一代。
- **stdlib 由当前自建 z42c（0.32，params 全支持）编译**，`params` 定义不死锁。
- **无 wire 格式变化**——纯签名重排 + 调用点 expanded form，**不 bump zbc/zpkg**。

## What Changes

- `String.Concat(params string[] values)` 单一重载取代 `Concat(a,b)` + `Concat(a,b,c)`。
  无分隔符逐个拼接；零实参 → `""`。
- `String.Format(string format, params object[] args)` 单一重载取代 `Format(fmt,arg0)` +
  `Format(fmt,arg0,arg1)`。循环把 `{i}` 替换为 `Convert.ToString(args[i])`；语义与原顺序替换等价
  （含「已替换文本再被后续 `{j}` 命中」的既有 footgun 不变）。
- 现有调用点 expanded form 源码兼容：`String.Concat("hello,","world")`（bench ×2）编译期打包
  `string[]`；无 `String.Format` 调用点。

## Scope（允许改动的文件 / 子系统）

占用子系统锁：`stdlib`（短占，归档即归还 `converge-z42c-onto-z42-project`；User 授权续做已合并的
params 迁移收尾）。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/String.z42` | MODIFY | `Concat`/`Format` → `params` |
| `src/libraries/z42.core/tests/*` | ADD/MODIFY | `Concat`/`Format` params 回归测试 |
| `docs/spec/archive/2026-07-01-add-params-varargs/tasks.md` | MODIFY | 阶段 9 候选勾除 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | stdlib 锁短占登记/归还 |

## 验证

- **本地**：two-gen 引导出 0.32 工具链后 `xtask build stdlib` + `xtask test stdlib z42.core`
  实跑 `Concat`/`Format` 新实现；e2e 若涉及。
- **GREEN 权威**：格式已在 main（0.32），但当前发布 nightly 仍 0.31 → 冷环境自举链以 CI 为准
  （ci-bootstrap 两代自举 / bootstrap-no-csharp）。
