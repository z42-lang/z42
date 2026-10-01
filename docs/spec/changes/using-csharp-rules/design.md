# Design: `using` 按 C# 规则（严格 + 多余 using 告警）

> 状态：User 2026-10-01 裁定，IMPL 中。出身：`enforce-fqn-name-identity` A 轴收口第 4 步（立门）的 DRAFT。

## User 裁定（2026-10-01）

1. **按 C# 规则**：外围命名空间隐式可见（`namespace A.B` 里可直接用 `A.B` / `A` 的成员），且**优先于** `using`。
   这推翻了 `enforce-fqn-name-identity/tasks.md` 里「z42 刻意没有父 ns 隐式可见」的结论（A5b-2b-1 为此补的
   `using Z42.IR;` 等会变成多余，PR-3 清掉）。
2. **严格**：除外围链与 prelude（`Std` / `Std.Runtime`）外，**一切**用到的命名空间都必须 `using`——
   **包括同包内的跨命名空间引用**（推翻参考手册「同包内不受此约束」那句）。
3. **多余的 `using` 报 warning**，三种都报：文件里没用到的；prelude / 本文件 ns（及外围）的；重复的（含与 `global using` 重复）。

## 可见集（唯一口径，`NsScope`）

文件 F（`namespace N`）可见的 ns = `Chain(N)`（由内到外）∪ F 的 `using`（含注入的 `global using`）∪ prelude ∪ 全局 ns。
查找优先级：`Chain(N)` 逐层（最内层胜出）→ `using` / prelude（多份 ⇒ E0456）。

## 分三个 PR

| PR | 内容 | 用户可见 |
|---|---|---|
| PR-1 `using-csharp-enclosing-ns` | `NsScope` + 解析器 ① / E0456 / 自由函数候选 / E0436 / 包激活 / static call 消歧 全部认外围链 | 放宽：外围 ns 不再需要 using；同名时外围胜出 |
| PR-2 立门 | 每文件收集「用到的 ns」；同包跨 ns 也须 using（E0436）；删自由函数「不可见也退到全集」那支；补仓库违规 | 收紧 |
| PR-3 多余 using 告警 | 新 warning 码（DiagnosticCodes 登记表逐个分配）；清理仓库多余 using | 新告警 |

## 关键约束

- PR-2 的门**挂在源码引用点**，不挂在解析器：A5b-2b-3 实测在解析器里判（落到裸名回落且 ns 不可见），stdlib 全量构建
  报 6496 条，绝大多数来自遍历类表、按名字查的 pass，不是源码引用。
- PR-3「没用到」依赖 PR-2 的「用到的 ns」集合完整；集合不完整 ⇒ 误报多余 ⇒ 用户删掉后才发现编不过。
- 驱动 `--emit-zbc` 单文件路径的包激活也要并外围链（`IrDump.ActivationNsOf`），但 driver→semantics 新符号需晚一个
  nightly（bootstrap-seed），留到 PR-1 之后。
