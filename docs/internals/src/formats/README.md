# 产物格式

> 本部分的概览页。各机制页见左侧目录。

zbc（字节码）、zpkg（包）、IR 是**编译器与运行时之间的协议**：z42c 产出、z42vm 消费。
两边的人都要查，所以独立成部分。

> **strict-pin**：z42vm 精确匹配 writer 的 major+minor，**没有跨版本兼容**。
> 格式 bump 的协调清单见 [version-bumping](https://github.com/z42-lang/z42/blob/main/.claude/rules/version-bumping.md)。
