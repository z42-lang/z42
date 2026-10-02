# z42.package

> 📌 **命名**：包名 `z42.package`、命名空间 `Z42.Package`。它装的**不只是 IR 模型**，而是「整个 package
> 文件的读写」—— IR 内存模型是包文件的**内容模型**，zbc 是它的编码，zpkg 是容器，三者是一件事。
> 真正的工程清单模型在 `z42.project` 包里（命名空间 `Z42.Project`）。

## 职责
编译栈**基础库**：IR 内存模型 + zbc 单模块字节码格式 + zpkg 包格式后端 + 类型导出/依赖索引。
z42c / z42b / 未来 REPL·分析工具经本库**共享**「emit IR → zbc/zpkg 读写」实现。无编译器逻辑（IrGen 留 z42c.semantics）。

## 核心文件
| 分组 | 文件 | 职责 |
|------|------|------|
| IR 模型 | `IrType` / `TypedReg` / `IrModule` / `IrInstr`（基类 + **统一操作数接口** `DefReg`/`ReadAt`/`StrAt`/`Clone` + 共享形状基类 `IrDefOnlyInstr`/`IrUnInstr`/`IrBinInstr`）+ 指令类按类别分文件 `IrInstrConst` / `IrInstrArith` / `IrInstrCall` / `IrInstrObject` / `IrTerminator`（`ReadReg`）/ `ObjectMethods` | 寄存器式 SSA IR（IrModule→IrFunction→IrBlock→IrInstr/Terminator）+ 类型标签 + 对象协议方法。**新增指令**：加 class 实现接口（操作数序 = REGT 访问序 = 池化序）+ `ZbcInstr.WriteInstr` / `ZbcReaderInstr` 编解码；REGT 收集、串池预扫、优化 pass 读写计数/改写、内联克隆、逃逸兜底自动覆盖（unify-ir-operand-access）。泛型方法（add-generic-methods）：`Call`/`VCall` 携 `MethodTypeArgs` + 新指令 `MethodTypeArgInsn`/`MethodDefaultInsn`（方法级 `typeof(T)`/`new T()`/`default(T)`，见 book「泛型方法」页）|
| 读侧线格式 | `ZpkgWire.z42`（`ZpkgCursor` / `ZpkgSection` / `SectionDir` / `StrsCodec` / `ConstraintCodec` / `SigsCodec` / `WireStr`）| `.zbc` / `.zpkg` / `.zsym` 三个读者（`ZbcReader` / `ZpkgReader` / `SidecarReader`）共用的**唯一**解码实现：游标、段目录、STRS 串池、型参约束包、SIGS 条目。写端对应物在 `ZbcWriter`（`BuildStrs` / `WriteSigEntries` / 约束包）。改线格式 = 改这里一处 + 写端一处（unify-package-codecs：此前三个读者各抄一份，约束包有三种读法、STRS 损坏有「拒绝 / 静默空池」两种反应）|
| zbc 格式 | `BinaryFormat/ByteWriter` / `ZbcFormat` / `ZbcStringPool` / `TokenAllocator` / `ZbcInstr` / `ZbcReader` / `ZbcReaderInstr` / `ZbcWriter` | byte-identical `.zbc` 写/读（8-section）+ 指令编解码 + 串池 + token 分配 |
| zpkg 后端 | `ZpkgWriter` / `ZpkgWriterIndexed` / `ZpkgReader` / `ZpkgBuilder` / `PackageTypes` / `TsigReconcile` | `.zpkg` 包格式读/写/构建 + 类型签名（TSIG）重建（含**本地 enum 导出** → 跨包 enum 导入，add-repl-decls-multiline）+ 包类型模型 |
| TSIG 重建索引 | `TsigIndex.z42`（`ReconClassIndex` / `SigsClassIndex`）| 类 FQ → (包,模块,类) 与 SIGS 按类分桶两张索引，把 `Rebuild` 的 world 全扫 / 祖先模块全扫从平方降为线性（perf-tsig-reconcile-index；25 包 world 下 TSIG 重建 ~0.94 s → 见 change 数据）|
| 惰性跨包类型世界 | `TsigReconcile.LazyReconWorld` | 按包懒填 TYPE/SIGS + 命名空间路由（`EnsureFq`）——`Rebuild` 基类链只解析引用闭包，不再一次性全量解析 world（lazy-type-world；O(引用) 不随库总量增长；旧 `BuildWorld`+4-arg `Rebuild` 作 eager 包装保留给种子）|
| 元数据 | `ExportedTypes` / `DependencyIndex` | 导出类型面 + 跨包依赖调用索引 |
| util | `StrMap` / `StrIndex` | `StrMap`：string→object 开放寻址 map（编译器全程用）；`StrIndex`：string→int 反查索引（无装箱、无删除），给插入序数组配 O(1) 查下标——`ZbcStringPool` / `IrGen` 字面量池用（perf-compiler-lookup-tables） |

## 入口点
`Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Package`（后者即 zpkg 后端，B3a 前叫 `Z42.Project`）。
IR 由 z42c.semantics 的 IrGen 构建；本库只提供模型 + 序列化。

## 依赖
z42.core（prelude）+ z42.encoding（Utf8）+ z42.io（zpkg 文件）+ z42.crypto（ZpkgBuilder 构建 id）。**无** z42c.* 反依赖（叶子）。

## 测试
`tests/smoke.z42`（自包含单元，3）· `tests/depindex.z42`（DependencyIndex，5）·
`tests/zpkg.z42`（zpkg 后端，4）——共 12 个 `[Test]`，均不依赖 IrGen。
完整 IR→zbc 往返（需 IrGen）在 `z42c.semantics/tests/zbcreader`。

```bash
xtask test stdlib z42.package          # 本库全部单元
xtask test stdlib z42.package -k zpkg  # 只跑一个
xtask test runtime                     # VM 读 tests/fixtures/ 的字节基线（zbc_compat、format_fixture_versions）
```

`tests/fixtures/{zbc-format,zpkg-format}/` 是本包两个 writer 的**签入字节基线**（不是 `[Test]` 单元）：
`.zbc` 一组由 `xtask build test` 就地重生，`git diff` 非空即格式漂移；`.zpkg` 一组按
[zpkg-format/README.md](tests/fixtures/zpkg-format/README.md) 的配方重生。格式 bump 的完整步骤见
[version-bumping.md](../../../docs/agent/rules/version-bumping.md)。

> 这三个单元为 flat 单文件（`tests/<name>.z42`）：stdlib 的 dir 单元发现要求目录里有 `source.z42`，
> 否则会被静默跳过（`test stdlib z42.package` 报「all 0 file(s) passed」）。
