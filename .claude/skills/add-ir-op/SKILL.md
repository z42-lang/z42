---
name: add-ir-op
description: 向 z42 IR 添加新的指令（opcode）。在用户说"新增 IR 指令"、"加操作码"、"实现 xxx 指令" 时触发。
user-invocable: true
allowed-tools: Read, Edit, Grep
argument-hint: <instruction-name>
---

# 添加 IR 指令：$ARGUMENTS

完整清单在 [docs/agent/playbooks/add-ir-op.md](../../../docs/agent/playbooks/add-ir-op.md)（与具体 AI 工具无关，
本文件只是它的斜杠命令包装）。先读它，**尤其开头「多数情况下你不该加 opcode」那一节**，再动手。
