# Proposal: REPL `.members <类型或变量>` 元指令

## Why
`.members`（类 Python `dir(x)`）列出一个类型/变量的成员——探索 API 的经典 REPL 内省命令，与 `.type`
（#134）成对。设计文档早列 `.members`，一直未接。当前只能靠 Tab 补全逐个看。

## What Changes
- **`.members <arg>` 元指令**（`interactive_main.z42`）：**复用补全器** `replComplete("<arg>.", …)`
  ——类型 → 静态成员、会话变量 → live 实例成员、未 reconcile 类型按需 reconcile（与 Tab/ghost 同源），
  逐行打印成员名。零新反射管线。
- **`Script.EnsureWarm(state)`**（`Script.z42`，public 包装私有 `_ensureWarm`）：`.members` 可能是**首命令**
  （无 eval 触发预热）→ 先汇合 `CachedScan` 就绪，否则 completer 读空 scan 返回空。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 派发加 `.members ` 分支；`_showMembers` 新函数；`_help` 补一行 |
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `EnsureWarm` public 包装（供元指令无 eval 时预热） |
| `docs/design/toolchain/repl.md` | MODIFY | 指令表 / 落地状态标 `.members` 已接 |

**只读引用**：
- `src/toolchain/scripting/src/Completer.z42` — `replComplete` 契约（`_typeStaticMembers` / `_memberComplete`）

## Out of Scope
- 命名空间名补全、`.mode`、错误行号回映、`.history`——留后续迭代。
- 成员分类展示（方法 vs 字段 vs 属性）——MVP 只列名（同 completer 输出）。

## Open Questions
- 无。
