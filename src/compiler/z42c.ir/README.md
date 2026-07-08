# z42c.ir

## 职责
镜像 C# [z42.IR](../../compiler/z42.IR/README.md)：共享契约（IR 内存模型 + zbc 二进制格式 + 项目类型）。与 C# 一致为**无依赖叶子**。CG-1A 起落地寄存器式 SSA **IR 内存模型**（IrModule → IrFunction → IrBlock → IrInstr/IrTerminator）；byte-identical `.zbc`（ZbcWriter）+ token 分配是后续独立 change。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/IrType.z42` | IR 基本类型标签（int 常量：I8..I64/U8..U64/F32/F64/Bool/Char/Str/Ref/Void）+ `Name(tag)`。堆对象塌缩为 Ref。`Z42Type→IrType` 映射不在此（叶子不引用 semantics），在 FunctionEmitter |
| `src/TypedReg.z42` | 带类型标签的虚拟寄存器 `TypedReg(Id, Type)` |
| `src/IrModule.z42` | 容器（叶子优先序）：IrFieldDesc / IrClassDesc / IrBlock / IrFunction / IrModule。集合 = typed array + count；StringPool（1-based）。Dump 出 .zasm-like 文本 |
| `src/IrInstr.z42` | IrInstr 基类 + virtual Dump() + 子类（class-per-instruction）：Const* + Copy + Add/Sub/Mul/Div/Rem + Call/VCall/FieldGet/FieldSet（CG-1C）+ ObjNew/ArrayGet/ArraySet/IsInstance/AsCast（CG-1D）+ Eq..Ge/BitAnd..Shr/Not/Neg/BitNot/StrConcat（CG-1E）。30+ 条，逼近 500 行将按类别拆 |
| `src/IrTerminator.z42` | IrTerminator 基类 + RetTerm/BrTerm/BrCondTerm/ThrowTerm（终结基本块） |
| `src/BinaryFormat/ByteWriter.z42` | byte-identical `.zbc` 字节缓冲（int[] 0..255 + LE WriteU8/U16/U32/I64/Str/Patch32/ToHex） |
| `src/BinaryFormat/ZbcFormat.z42` | .zbc 格式常量（ZbcVersion（随 C# 同步，见 version-bumping.md 第 5 步）/ Op / Tag / ExecMode）+ Tag.FromName/FromIrType（**IrType 序≠zbc tag 序，显式映射**） |
| `src/BinaryFormat/ZbcStringPool.z42` | .zbc 字符串池（插入序 intern + idx 查找，0-based；STRS 字节序 = intern 序） |
| `src/BinaryFormat/TokenAllocator.z42` | token 分配（ZW-1C）：FromModule 插入序 name→index；跨模块 = `ImportBase(1<<31)\|STRS idx`；VCall/Field 不 token 化 |
| `src/BinaryFormat/ZbcInstr.z42` | 指令/终结符字节编码（集中 if-is，镜像 C# WriteInstr/WriteTerminator）：const(int/f64[IEEE754 bits]/bool/null/str)/copy/算术/比较/位/一元/concat + call/vcall/field/obj_new/is·as/array + ret/retval/br/brcond/throw + InternStrings 预扫 |
| `src/BinaryFormat/ZbcWriter.z42` | `IrModule → .zbc 字节`（byte-identical vs C# ZbcWriter）：intern 预扫 + 全 8-section（NSPC/STRS/TYPE/SIGS/IMPT/EXPT/FUNC/REGT）+ header/directory 组装。ZW-1A：trivial 函数（`empty` 逐字节对账通过） |
| `src/BinaryFormat/ZbcReader.z42` | fullMode `.zbc → IrModule` 读取器（add-file-level-incremental D5；对照 C# 历史版移植）：strict-pin + 段解码（NSPC/STRS/TYPE/SIGS/FUNC/REGT/DBUG/TIDX）+ ConstStr 模块池重建；占位 label（真实 label 等 writer 残留由 cache meta 回填——D5a）；坏输入/未知 op → null。往返单测 `tests/zbcreader/`（Write→Read→Write 字节相等） |
| `src/BinaryFormat/ZbcReaderInstr.z42` | 指令/终结符解码（镜像 ZbcInstr 布局）+ REGT 类型回填 `Retype`（tag 兼容升级 + 纯操作数取 REGT——BuildRegt 复现同字节）；⚠ token bit31 / u32 哨兵须显式位检测（z42 int 移位下不为负） |
| `src/IrSkeleton.z42` | B0 占位（暂留：SemanticsSkeleton/ProjectSkeleton/PipelineSkeleton 仍引用；随其移除时清理） |

## 入口点
`Z42.IR` 命名空间。IR 由 z42c.semantics 的 IrGen/FunctionEmitter 构建；文本 dump 经 `IrModule.Dump()` / `IrFunction.Dump()`。

## 依赖关系
无（叶子；与 C# z42.IR 一致）。stdlib 自动可用。

## 受限写法
class（非 record）+ virtual Dump 替 record 层次；static class + int 常量替 enum（IrType）；typed array + count 替泛型集合字段。**定义顺序叶子优先**（容器引用叶子的 Dump，bootstrap 单遍按文件序解析，后定义的具体类型方法不可见）。

## 增量进度
CG-1A 最小指令集 ✅ / CG-1B 控制流 ✅ / CG-1C 调用·字段 ✅ / CG-1D new·数组·is·as ✅ / CG-1E 运算符 + 块化短路·三目·?? ✅ / CG-2 泛型 ✅。**Bound→IR 内存模型 codegen 覆盖全部非泛型 L1 + 泛型实例化**。byte-identical `.zbc`（`BinaryFormat/`，change `port-z42c-zbc-writer`）：**ZW-1A `empty` 逐字节对账 ✅（zbc 1.12）/ ZW-1B 运算 opcode + driver `--emit-zbc` + z42vm 端到端执行 ✅**（自检程序经自举字节正确运行——自举里程碑）。🔴 全面 byte-identical 阻塞于 DBUG section（C# 对有语句体函数 emit 源码行表，z42c AST 无 span）—— 待 span→LineTable→DBUG 链（ZW-1C+）。
