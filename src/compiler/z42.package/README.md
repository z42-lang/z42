# z42.package

## 职责
编译栈**基础库**：IR 内存模型 + zbc 单模块字节码格式 + zpkg 包格式后端 + 类型导出/依赖索引。
z42c / z42b / REPL·分析工具经本库**共享**「emit IR → zbc/zpkg 读写」实现。无编译器逻辑（IrGen 在 `z42c.emission`）。

包名 `z42.package`、命名空间 `Z42.Package`：它装的是「整个 package 文件的读写」——IR 内存模型是包文件的
**内容模型**，zbc 是它的编码，zpkg 是容器。工程清单模型在 `z42.project`（命名空间 `Z42.Project`）。

## 功能索引
命名空间：`Z42.IR`（IR 模型）/ `Z42.IR.BinaryFormat`（zbc 编解码）/ `Z42.Package`（zpkg 后端）。
IR 由 `z42c.emission` 的 IrGen 构建；本库只提供模型 + 序列化。

## 如何测试验证
`tests/` 下为 flat 单文件单元（`tests/<name>.z42`；stdlib 的 dir 单元发现要求目录里有 `source.z42`，
否则会被静默跳过，报「all 0 file(s) passed」），均不依赖 IrGen。完整 IR→zbc 往返（需 IrGen）在
`z42c.emission/tests/zbcreader`。

```bash
./xtask test stdlib z42.package          # 本库全部单元
./xtask test stdlib z42.package -k zpkg  # 只跑一个
./xtask test runtime                     # VM 读 tests/fixtures/ 的字节基线（zbc_compat、format_fixture_versions）
```

`tests/fixtures/{zbc-format,zpkg-format}/` 是两个 writer 的**签入字节基线**（不是 `[Test]` 单元）：
`.zbc` 一组由 `xtask build test` 就地重生，`git diff` 非空即格式漂移；`.zpkg` 一组按
[zpkg-format/README.md](tests/fixtures/zpkg-format/README.md) 的配方重生。格式 bump 的完整步骤见
[version-bumping.md](../../../docs/agent/rules/version-bumping.md)。

## 关联文档
- 格式 bump 流程：[version-bumping.md](../../../docs/agent/rules/version-bumping.md)
- 设计 / 机制：[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)

## 核心文件
| 分组 | 文件 | 职责 |
|------|------|------|
| IR 模型 | `IrType` / `TypedReg` / `IrModule` / `IrInstr`（基类 + **统一操作数接口** `DefReg`/`ReadAt`/`StrAt`/`Clone` + 编码 `Write` / 头部 tag 来源 `TagReg` / 可删性 `IsPure` + 共享形状基类 `IrDefOnlyInstr`/`IrUnInstr`/`IrBinInstr`）+ 指令类按类别分文件 `IrInstrConst` / `IrInstrArith` / `IrInstrCall` / `IrInstrObject` / `IrTerminator`（`ReadReg`）/ `ObjectMethods` | 寄存器式 SSA IR（IrModule→IrFunction→IrBlock→IrInstr/Terminator）+ 类型标签 + 对象协议方法。**新增指令**：加 class 实现接口（操作数序 = REGT 访问序 = 池化序；`Write` 编码）+ `ZbcReaderInstr` 解码；REGT 收集与读端回填、串池预扫、优化 pass 读写计数/改写、内联克隆、逃逸兜底自动覆盖。泛型方法：`Call`/`VCall` 携 `MethodTypeArgs`，另有 `MethodTypeArgInsn`/`MethodDefaultInsn`（方法级 `typeof(T)`/`new T()`/`default(T)`） |
| 读侧线格式 | `ZpkgWire.z42`（`ZpkgCursor` / `ZpkgSection` / `SectionDir` / `StrsCodec` / `ConstraintCodec` / `SigsCodec` / `WireStr`）| `.zbc` / `.zpkg` / `.zsym` 三个读者（`ZbcReader` / `ZpkgReader` / `SidecarReader`）共用的**唯一**解码实现：游标、段目录、STRS 串池、型参约束包、SIGS 条目。写端对应物在 `ZbcWriter`（`BuildStrs` / `WriteSigEntries` / 约束包）。改线格式 = 改这里一处 + 写端一处 |
| zbc 格式 | `BinaryFormat/ByteWriter` / `ZbcFormat` / `ZbcStringPool` / `TokenAllocator` / `ZbcEncoder` / `ZbcInstr` / `ZbcReader` / `ZbcReaderInstr` / `ZbcWriter` | 确定性的 `.zbc` 写/读（8-section）+ 指令编解码（`ZbcEncoder` = 单条指令的编码上下文，供 `IrInstr.Write`）+ 串池 + token 分配 |
| zpkg 后端 | `ZpkgWriter` / `ZpkgWriterIndexed` / `ZpkgReader` / `ZpkgBuilder` / `PackageTypes` / `TsigReconcile` | `.zpkg` 包格式读/写/构建 + 类型签名（TSIG）重建（含本地 enum 导出 → 跨包 enum 导入）+ 包类型模型 |
| sidecar | `SidecarReader` | 读 `.zsym` SymOnly sidecar（META + STRS + MDBG + BLID），建 frame-name → 行表索引，供离线符号化 |
| TSIG 重建索引 | `TsigIndex.z42`（`ReconClassIndex` / `SigsClassIndex`）| 类 FQ → (包,模块,类) 与 SIGS 按类分桶两张索引，使 `Rebuild` 的 world 扫描与祖先模块扫描为线性 |
| 惰性跨包类型世界 | `TsigReconcile.LazyReconWorld` | 按包懒填 TYPE/SIGS + 命名空间路由（`EnsureFq`）——`Rebuild` 基类链只解析引用闭包，开销随引用数而非库总量增长 |
| 元数据 | `ExportedTypes` / `DependencyIndex` | 导出类型面 + 跨包依赖调用索引 |
| util | `StrMap` / `StrIndex` / `Murmur3` | `StrMap`：string→object 开放寻址 map（编译器全程用）；`StrIndex`：string→int 反查索引（无装箱、无删除），给插入序数组配 O(1) 查下标，`ZbcStringPool` / `IrGen` 字面量池用；`Murmur3`：MurmurHash3 x86_128，zpkg 内部内容标识（BLID build_id、源文件变更检测），非密码学用途 |

## 依赖关系
`z42.core`（prelude）+ `z42.encoding`（Utf8）+ `z42.io`（zpkg 文件）+ `z42.crypto`（`ZpkgBuilder` 的 Sha256Hex 入口）。**无** `z42c.*` 反依赖（叶子）。
