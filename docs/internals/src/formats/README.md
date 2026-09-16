# 产物格式

zbc（字节码）、zpkg（包）、IR —— **编译器与运行时之间的协议**：z42c 产出、z42vm 消费。
两边的人都要查，所以独立成部分。

| 格式 | 是什么 | 权威实现 |
|---|---|---|
| [zbc](zbc.md) | 单个编译单元的字节码 | `z42.ir/src/ZbcWriter.z42` ↔ `src/runtime/src/metadata/zbc_reader/` |
| [zpkg](zpkg.md) | 一个包（多个 zbc + 元数据 + TSIG + IMPL） | `z42.ir/src/ZpkgWriter.z42` ↔ `ZpkgReader.z42` |
| [IR](ir.md) | 指令集与类型映射 | `z42.ir/src/Ir*.z42` |

## strict-pin：没有跨版本兼容

**z42vm 精确匹配 writer 的 major + minor**，不匹配直接拒读——没有 fallback、没有兼容层。
残留旧产物用 `xtask build test` 重生。

> 格式 bump 的完整 checklist（改哪些文件、commit 前自检命令、两代自举怎么过）见
> [version-bumping.md](../../../agent/rules/version-bumping.md)。
> **每次 bump 必须在 [zbc.md](zbc.md) 的 Minor changelog 表加一行。**
