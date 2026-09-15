---
paths:
  - "docs/design/**/*.md"
---

# 语言规范编写规范

## 规范文件职责

| 文件 | 内容 |
|------|------|
| `docs/design/language/language-overview.md` | 语法、类型系统、所有权模型、并发模型的 **用户视角** 描述 |
| `docs/design/runtime/ir.md` | IR 指令集、类型映射、二进制格式的 **实现者视角** 描述 |

## 修改原则

- 语法变更必须同时更新代码示例块（保持可运行 / 可解析）
- 新增 IR 指令必须包含：指令名、操作数类型、语义描述、伪代码示例
- 规范中的示例代码以 ` ```z42 ` 代码块标注，IR 示例以 ` ```  ` 裸代码块标注

## 仓库根 examples/ 不是特性示例库

`examples/` 只放学习手册（`docs/learn/`）的配套工程，结构镜像手册页面，规则见
[`docs/agent/rules/learn-writing.md`](../../docs/agent/rules/learn-writing.md)。
新特性的覆盖写成测试（`src/tests/` / 库的 `tests/`），不要往 `examples/` 加演示文件。
