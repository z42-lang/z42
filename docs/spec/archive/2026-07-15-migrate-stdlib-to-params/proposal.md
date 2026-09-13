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

**顺带 dedup**：`Path.Combine(a,b)` 是 `Path.Join(a,b)` 的 BCL 兼容别名（body 仅
`return Path.Join(a, b)`）。`Path.Join` 已在 #7 落地 `params`，`Combine` **全库 0 调用点**
（`Path.Join` 286 处）→ 冗余死别名，删除。

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
| `src/libraries/z42.io/src/Path.z42` | MODIFY | 删死别名 `Path.Combine`（0 调用点） |
| `src/libraries/z42.core/tests/*` | ADD/MODIFY | `Concat`/`Format` params 回归测试 |
| `docs/spec/archive/2026-07-01-add-params-varargs/tasks.md` | MODIFY | 阶段 9 候选勾除 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | stdlib 锁短占登记/归还 |

## 已知限制（`string.Concat()` 零实参 → 独立编译器缺陷，非本变更引入）

`string.Concat()`（**纯 params 方法、零可变实参**）在运行期崩溃（interp `undefined register`
/ jit `type mismatch Null vs Null`）。clean 0.32 工具链实测的**精确边界**（22/22 对照用例）：

| 用例 | 结果 | 说明 |
|------|------|------|
| `string.Concat()` | ❌ FAIL | 纯 params + 零实参 → 合成**空数组字面量作唯一实参** |
| `string.Join("-")`（仅 sep、零 values） | ✅ PASS | 有固定前缀形参，空数组作**第 2** 实参 |
| `string.Concat(xs)`（xs 空数组变量，normal form） | ✅ PASS | 直传变量，不合成字面量 |
| `new string[]{}`（显式空数组） | ✅ PASS | 显式空数组字面量 codegen 正常 |
| 所有非空 params（1/N 实参、混类型、重复占位符） | ✅ PASS | interp+jit 全绿 |

根因缩小到：`_withParamsExpansion`（OverloadBinder.z42:48-56）为零实参合成
`BoundArrayLit(elemCount=0)`，当它作为**静态调用的唯一/首个实参**（纯 params，无固定前缀）
被 emit 时 codegen/VM 出错（register %0 未定义）。`Join`（有 sep 前缀）与显式空数组均正常
→ 缺陷**不在** #7 的 `Join`、也**不是**一般空数组 codegen。

- **非本变更引入**：迁移前 `Concat()` 无匹配固定-arity 重载 → 直接编译错误；迁移后才可表达该
  退化调用（拼接零段 → `""`），从而**暴露**（非造成）此既有编译器缺陷。
- **归属 `compiler`/`runtime` 子系统**，非 stdlib；越出本变更 Scope。→ 单列独立编译器 change
  修复（`docs/roadmap.md` Deferred `params-future-empty-array-codegen`；待 compiler 锁空闲）。
  回归用例 `string_params_methods` 覆盖全部工作情形并注明此单点限制。

## 验证

- **本地**：two-gen 引导出 0.32 工具链后 `xtask build stdlib` + `xtask test stdlib z42.core`
  实跑 `Concat`/`Format` 新实现；e2e 若涉及。
- **GREEN 权威**：格式已在 main（0.32），但当前发布 nightly 仍 0.31 → 冷环境自举链以 CI 为准
  （ci-bootstrap 两代自举 / bootstrap-no-csharp）。
