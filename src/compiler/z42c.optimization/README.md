# z42c.optimization

## 职责
编译期 **IR → IR 优化管线**：常量折叠 / 复制传播 / CSE / LICM / 内联 / 逃逸分析栈上分配 / 循环内分配复用 /
死分支消除 / 纯函数推断，以及决定开哪些优化的具名位集 `Opt`。只依赖 IR 模型（`z42.package`），不碰语法树、
符号表或 Bound 树——输入输出都是 `IrModule`。

**不做**：Bound 树 → IR 的代码生成（[`z42c.semantics`](../z42c.semantics/README.md) 的 `IrGen`）；
清单 / 命令行开关的解析编排（`z42c.pipeline` 的 `ManifestKnobs`、`z42c.driver`，它们只调本包的 `Opt.*`）。

## 功能索引
| 能力 | 入口 |
|---|---|
| 跑优化管线 | `IrOptPipeline.Run(m, optSet)`（`z42c.semantics` 的 `IrGen.Generate` 末尾调用；`Opt.None` = -O0 整体跳过） |
| 优化开关 | `Opt.ConstFold` / `CopyProp` / `Inline` / … / `All`、`Opt.Has` / `ByName` / `ProfileDefault` / `Resolve` / `FromToml` |
| IR 读写分析 | `IrOptInfo`（定义 / 读操作数 / 改写 / 纯度 / 常量折叠）、`IrRegCounts` |

## 如何测试验证
本包无独立 `tests/`：优化的正确性靠「源码 → IrGen → 优化」整条链断言，住
[`z42c.semantics/tests/codegen`](../z42c.semantics/tests/codegen/)（按 `Opt.*` 逐位开关对拍）与 golden
`src/tests/optimization/`（必须带 `opt_all` 侧车）。
```bash
./xtask test compiler        # 自举不动点 + 编译器各包单测（含 codegen）
./xtask test e2e --dir optimization
```

## 关联文档
- 机制：[优化管线](../../../docs/internals/src/runtime/optimization-pipeline.md)、[逃逸分析](../../../docs/internals/src/runtime/escape-analysis.md)
- 命令行 / 清单开关：[z42c / z42b CLI](../../../docs/reference/src/toolchain/cli-z42c-z42b.md)

## 核心文件
| 文件 | 职责 |
|---|---|
| `src/IrOptInfo.z42` | **IR 优化基石**：逐 opcode 的写寄存器 `DstId`/`AddDef` / 读寄存器 `AddReads`+`AddTermReads`（经 z42.package 统一操作数接口 `DefReg`/`ReadAt`/`ReadReg` 枚举）/ **读操作数重写 `ReplaceReads`+`ReplaceTermReads`**（按 remap 改写读操作数，供 use-site copy-prop / CSE 复用）/ **CSE value-number `CseKey`+`DstReg`**（纯计算 op 的 `op|操作数ids` key + dst 提取）/ 可删性 `IsPure`（白名单，未知 opcode 保留）/ retarget `SetDst`（copy-prop）/ `TryConstFold`（const-fold 规则表，可扩展） |
| `src/OptSet.z42` | **可独立开关的具名优化位集**（`Opt` static class）：`ConstFold=1/CopyProp=2/Dce=4/Inline=8/Cse=16/Licm=32/StackAlloc=64/LoopAllocReuse=128/ReadonlyLoad=256/PureCall=512/DeadBranch=1024/All=2047` + `Has`/`ByName`/`ProfileDefault(isRelease)`（debug=None/-O0、release=All）/`Resolve`（CLI>toml>profile）|
| `src/IrDeadBranch.z42` | **常量条件死分支消除 pass**（`Opt.DeadBranch`）：单赋值 `ConstBoolInstr` 条件的 `br.cond`→无条件 `br` 折叠 + `ExcCount==0` 时可达性 BFS 移不可达块。**`ExcCount>0` 只折不移**（CFG 铁律：异常隐式边不在终结子 CFG，镜像 IrLicm 跳过）。见 book optimization-pipeline |
| `src/IrPureFunctionTable.z42` | **纯函数推断**：`PureTable`（funcName 集）+ `Compute(m)` 模块**单调不动点**（与 escape 相反：乐观全纯→发现副作用/读可变/抛/调非纯→降级→收敛；StrMap 无 Remove 故每轮重建）。`pure(f)`=每指令 IsPure∪对纯函数 call∪readonly-fget 且无 throw 终结。供 CSE/LICM 判纯调用可消重/外提。无体/imported→保守非纯 |
| `src/IrEscapeSummary.z42` | **跨过程参数逃逸摘要**：`ParamEscapeTable`（funcName→`ParamFlags(bool[ParamCount])`，参数槽含 this=槽0）+ `Compute(m)` 模块**单调不动点**（乐观全 false→逃逸的置 true→收敛）。供 IrEscapeAnalysis 把「传进静态调用的实参」从「一律逃逸」精确成「按 callee 摘要逐判」。无体 stub 不登记→调用点保守 |
| `src/IrEscapeAnalysis.z42` | **逃逸分析栈上分配 pass**（`Opt.StackAlloc`，入 All）：CFG-free 流不敏感 may-escape 过近似——`ComputeEscapedRegs(m,f,table)`（Pass A 角色感知逃逸汇点规则表 `_markEscaping` 入种子 + Pass B copy 传递闭包）。**跨过程摘要**：`CallInstr`(args[i]→槽 i)/`ObjNew`(args[i]→ctor 槽 i+1) 实参按 `ParamEscapeTable` 逐判；VCall/CallIndirect/builtin/闭包/跨包保守全标。对象合格前提 = ctor 摘要槽 0 不逃逸（原 `_ctorLeaksThis` 并入）。不逃逸+单赋值 `ObjNew`/`ArrayNew`/`ArrayNewLit`→`StackAlloc=true`。见 book escape-analysis-stack-alloc |
| `src/IrLoopUtil.z42` | **自然循环分析共享机件**（供 IrLicm + IrLoopAllocReuse）：`LoopCfg`（后继/前驱/支配）+ `BuildCfg`（<2 块 / 有异常表 → null）+ `Headers`（回边目标）+ `LoopBody`（并同 header 多回边体）+ `CleanPreheader`（唯一循环外 `br h` 前驱）+ `BlockIdx`。从 IrLicm 抽出，逐字节等价 |
| `src/IrLicm.z42` | **循环不变量外提 pass**（`Opt.Licm`，入 All）：CFG/循环机件复用 `IrLoopUtil` + 不变量（IsPure + 单赋值 dst + 操作数不在 **header 支配域**内定义）+ 外提。**跳过有异常表的函数**（`ExcCount>0`——CFG 不含异常隐式边）。`Run(f, optSet)` 增 `_isHoistableReadonlyFget` 分支（`Opt.ReadonlyLoad` 门控）——接收者 `this`（reg0 恒非空）的 readonly `field_get` + 循环体内该字段无 `field_set` → 外提。`Run(f, optSet, pureTable)` 增 `_isHoistablePureCall` 分支（`Opt.PureCall` 门控）——纯 `CallInstr`（callee 在纯表、含 no-throw）+ args 全循环不变 → 外提。见 book optimization-pipeline |
| `src/IrLoopAllocReuse.z42` | **循环内分配 hoist + 对象复用 pass**（`Opt.LoopAllocReuse=128`，入 All；escape 之后）：复用 `IrLoopUtil`，把循环体内**迭代内可复用**的 `ObjNew`/`ArrayNew`（C1 StackAlloc + C2 前向 copy 闭包无多赋值 = 不跨迭代携带 + C3 数组 Size 循环不变 + C4 对象 ctor 单块/数组常量下标读前写全）hoist 到 pre-header 只分配一次 + 循环体重初始化（对象=空 ctor 名裸分配 + `Call ctor(%r,args)`；数组=整条移走）。无格式 bump。主正确性门=`--no-opt loop-alloc-reuse` 开/关对拍 |
| `src/IrRegCounts.z42` | **寄存器读/写计数单一实现**（`Defs` / `DefsCap` / `Reads`，扫全函数累加 `IrOptInfo.AddDef`/`AddReads`/`AddTermReads`）；IrOptPipeline / IrDeadBranch / IrEscapeAnalysis / IrLoopAllocReuse 共用。 |
| `src/IrOptPipeline.z42` | **编译期 IR 优化管线**（IrGen.Generate 末尾）：`Run(m, optSet)` 按 `Opt.Has` 门控每 pass（`None`→整体跳过=-O0）。先模块级 inline（`IrInline.Run`，靠前产更多下游机会），再逐函数 const-fold → **licm**（`IrLicm.Run` 循环不变量外提）→ **cse**（`_passCse` 块内 value-number 去重）→ copy-prop（producer-retarget + **use-site 级联** `_passCopyPropUse`，靠 `ReplaceReads`）→ temp-DCE。licm/cse 两 pass 的触发改为 `Licm||ReadonlyLoad` / `Cse||ReadonlyLoad`，readonly `field_get` 消重/外提分支由 `ReadonlyLoad` 位单独门控（CSE 侧 `_collectWrittenFields` 失效被写字段）。`Run` 在 per-函数 pass 前算 `IrPureFunctionTable.Compute(m)` 传下去；licm/cse 触发再加 `||PureCall`，纯 `CallInstr` 消重/外提由 `PureCall` 位门控（纯调用无需失效表）。interp-first，见 book optimization-pipeline |
| `src/IrInline.z42` | **函数内联 pass**（`Opt.Inline`）：模块级逐 caller 展开合格直接调用点。curated 集（const/copy/算术/比较/位·一元/convert/field_get），callee 非递归/无异常表·varargs/精确 arity；offset=caller.MaxReg 重映射寄存器 + reg_types 同步扩 + 稳定序（自举不动点）。**只读形参直代入实参寄存器**（`InlineCtx`/`_writtenParamsAll`，免 param copy）。**Phase A 单块就地 splice + Phase B 多块 split+insert**（`InlineState`/`_spliceMultiBlock`/`_cloneCalleeBlock`，含控制流 callee 拆块内联、唯一 relabel、Ret→续延块）。资格/展开 |

## 依赖关系
`z42.package`（IR 模型 `Z42.IR`；常量折叠复用 `ZbcInstr._parseIntLit` 的权威字面量解析）。
被依赖：`z42c.semantics`（`IrGen` 调管线、`Opt` 供 devirt 门控）、`z42c.pipeline`（`ManifestKnobs` / `PackageCompile` 解析开关）、
`z42c.driver`（`--opt` / `--no-opt` / `--opt-all`）。
