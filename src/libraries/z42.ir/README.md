# z42.ir

## 职责
编译栈**基础库**：IR 内存模型 + zbc 单模块字节码格式 + zpkg 包格式后端 + 类型导出/依赖索引。
z42c / z42b / 未来 REPL·分析工具经本库**共享**「emit IR → zbc/zpkg 读写」实现（converge-z42c-ir-metadata-onto-stdlib：
从旧 `z42c.ir` + `z42c.project`(zpkg 后端) 下沉，沿用 `z42.project` 收敛范式）。无编译器逻辑（IrGen 留 z42c.semantics）。

## 核心文件
| 分组 | 文件 | 职责 |
|------|------|------|
| IR 模型 | `IrType` / `TypedReg` / `IrModule` / `IrInstr`（基类 + **统一操作数接口** `DefReg`/`ReadAt`/`StrAt`/`Clone` + 共享形状基类 `IrDefOnlyInstr`/`IrUnInstr`/`IrBinInstr`）+ 指令类按类别分文件 `IrInstrConst` / `IrInstrArith` / `IrInstrCall` / `IrInstrObject` / `IrTerminator`（`ReadReg`）/ `ObjectMethods` | 寄存器式 SSA IR（IrModule→IrFunction→IrBlock→IrInstr/Terminator）+ 类型标签 + 对象协议方法。**新增指令**：加 class 实现接口（操作数序 = REGT 访问序 = 池化序）+ `ZbcInstr.WriteInstr` / `ZbcReaderInstr` 编解码；REGT 收集、串池预扫、优化 pass 读写计数/改写、内联克隆、逃逸兜底自动覆盖（unify-ir-operand-access）。泛型方法（add-generic-methods）：`Call`/`VCall` 携 `MethodTypeArgs` + 新指令 `MethodTypeArgInsn`/`MethodDefaultInsn`（方法级 `typeof(T)`/`new T()`/`default(T)`，见 book「泛型方法」页）|
| zbc 格式 | `BinaryFormat/ByteWriter` / `ZbcFormat` / `ZbcStringPool` / `TokenAllocator` / `ZbcInstr` / `ZbcReader` / `ZbcReaderInstr` / `ZbcWriter` | byte-identical `.zbc` 写/读（8-section）+ 指令编解码 + 串池 + token 分配 |
| zpkg 后端 | `ZpkgWriter` / `ZpkgWriterIndexed` / `ZpkgReader` / `ZpkgBuilder` / `PackageTypes` / `TsigReconcile` | `.zpkg` 包格式读/写/构建 + 类型签名（TSIG）重建（含**本地 enum 导出** → 跨包 enum 导入，add-repl-decls-multiline）+ 包类型模型 |
| 惰性跨包类型世界 | `TsigReconcile.LazyReconWorld` | 按包懒填 TYPE/SIGS + 命名空间路由（`EnsureFq`）——`Rebuild` 基类链只解析引用闭包，不再一次性全量解析 world（lazy-type-world；O(引用) 不随库总量增长；旧 `BuildWorld`+4-arg `Rebuild` 作 eager 包装保留给种子）|
| 元数据 | `ExportedTypes` / `DependencyIndex` | 导出类型面 + 跨包依赖调用索引 |
| util | `StrMap` / `StrIndex` | `StrMap`：string→object 开放寻址 map（编译器全程用）；`StrIndex`：string→int 反查索引（无装箱、无删除），给插入序数组配 O(1) 查下标——`ZbcStringPool` / `IrGen` 字面量池用（perf-compiler-lookup-tables） |

## 入口点
`Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Project`（zpkg 后端沿用旧 namespace，纯 MOVE 无并存）。
IR 由 z42c.semantics 的 IrGen 构建；本库只提供模型 + 序列化。

## 依赖
z42.core（prelude）+ z42.encoding（Utf8）+ z42.io（zpkg 文件）+ z42.crypto（ZpkgBuilder 构建 id）。**无** z42c.* 反依赖（叶子）。

## 测试
`tests/smoke`（自包含单元）· `tests/depindex`（DependencyIndex）· `tests/zpkg`（zpkg 后端）——均不依赖 IrGen。
完整 IR→zbc 往返（需 IrGen）在 `z42c.semantics/tests/zbcreader`。
