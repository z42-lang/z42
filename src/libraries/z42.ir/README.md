# z42.ir

## 职责
编译栈**基础库**：IR 内存模型 + zbc 单模块字节码格式 + zpkg 包格式后端 + 类型导出/依赖索引。
z42c / z42b / 未来 REPL·分析工具经本库**共享**「emit IR → zbc/zpkg 读写」实现（converge-z42c-ir-metadata-onto-stdlib：
从旧 `z42c.ir` + `z42c.project`(zpkg 后端) 下沉，沿用 `z42.project` 收敛范式）。无编译器逻辑（IrGen 留 z42c.semantics）。

## 核心文件
| 分组 | 文件 | 职责 |
|------|------|------|
| IR 模型 | `IrType` / `TypedReg` / `IrModule` / `IrInstr` / `IrTerminator` / `ObjectMethods` | 寄存器式 SSA IR（IrModule→IrFunction→IrBlock→IrInstr/Terminator）+ 类型标签 + 对象协议方法 |
| zbc 格式 | `BinaryFormat/ByteWriter` / `ZbcFormat` / `ZbcStringPool` / `TokenAllocator` / `ZbcInstr` / `ZbcReader` / `ZbcReaderInstr` / `ZbcWriter` | byte-identical `.zbc` 写/读（8-section）+ 指令编解码 + 串池 + token 分配 |
| zpkg 后端 | `ZpkgWriter` / `ZpkgWriterIndexed` / `ZpkgReader` / `ZpkgBuilder` / `PackageTypes` / `TsigReconcile` | `.zpkg` 包格式读/写/构建 + 类型签名（TSIG）重建 + 包类型模型 |
| 元数据 | `ExportedTypes` / `DependencyIndex` | 导出类型面 + 跨包依赖调用索引 |
| util | `StrMap` | 有序 map（确定性迭代；编译器全程用） |

## 入口点
`Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Project`（zpkg 后端沿用旧 namespace，纯 MOVE 无并存）。
IR 由 z42c.semantics 的 IrGen 构建；本库只提供模型 + 序列化。

## 依赖
z42.core（prelude）+ z42.encoding（Utf8）+ z42.io（zpkg 文件）+ z42.crypto（ZpkgBuilder 构建 id）。**无** z42c.* 反依赖（叶子）。

## 测试
`tests/smoke`（自包含单元）· `tests/depindex`（DependencyIndex）· `tests/zpkg`（zpkg 后端）——均不依赖 IrGen。
完整 IR→zbc 往返（需 IrGen）在 `z42c.semantics/tests/zbcreader`。
