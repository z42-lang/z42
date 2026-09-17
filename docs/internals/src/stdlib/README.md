# 标准库

> 对齐：2026-09-17（change `restructure-docs-three-books`）

改标准库本身时要读的东西：包怎么分、native 能力放哪儿、写 API 守什么准则。

**各包的公开 API 不在这里**——那是[语言与库参考的标准库部分](../../../reference/src/stdlib/README.md)。
本部分只回答「为什么这么分、在哪改、加一个包要满足什么」。

| 页 | 什么时候读 |
|---|---|
| [三层架构](architecture.md) | 想知道 stdlib 整体怎么分层、extern 预算怎么算 |
| [包划分与 interop 归属](organization.md) | **要新开一个包**，或要决定一个 `[Native]` 符号声明在哪个包 |
| [API 准则](api-guidelines.md) | 要给现有包加公开方法 |
| [JSON serde 的反射底座](json-serde.md) | 要改 `JsonSerializer` 的绑定行为，或改动反射 API 时要知道谁在依赖它 |

各包目录自身的结构说明见 `src/libraries/<pkg>/README.md`。
