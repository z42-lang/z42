# Tasks: enum 成为独立类型

> 状态：🔴 DRAFT 待 User 确认 | 创建：2026-09-07
>
> **来源**：User 在 `add-argument-type-check` 的 Q2 裁决中选了「C# 语义」。因其为**语言语义变更**、
> 半径远超实参检查，按 [workflow.md](../../../../.claude/rules/workflow.md) Spec-First 拆为本独立 lang change。
>
> **依赖**：`add-argument-type-check` 先合（它已把 enum 位跳过并留了指向本变更的注释）。
> 本变更**必须摘掉**那个跳过（阶段 3.1），否则 enum 位永远不检查。
>
> 🔴 **未获 6.5 确认前不得写实现代码。**

## 进度概览
- [x] 阶段 0: 摸清未知（codegen / 装箱 / 跨包 enum）——**已完成**，结论见 design D4
- [x] 阶段 1: enum 类型表示（`IsEnum` 标记）—— 1.1/1.2/1.3 完成；**1.5 装箱未做**
- [x] 阶段 2: 转换与运算符 —— 全部完成
- [ ] 阶段 3: 摘跳过 + 调用点/测试改写 —— 3.1/3.2 完成，3.3–3.5 待办
- [ ] 阶段 4: 验证与归档

## 🔧 开工前的前提修正（2026-09-07，`add-argument-type-check` 合并后核查 main 得出）

**proposal / design 起草时低估了 main 已有的 enum 基础设施。以下为核实结果，阶段 0 据此收窄：**

| 我起草时的假设 | 实际（`origin/main` 核实） |
|---|---|
| enum 反射 / 底层类型属 Out of Scope、"另议" | ❌ **已存在**：`Type.GetEnumUnderlyingType()`（`add-enum-underlying-type`，`[Native("__type_enum_underlying")]`）+ `z42.core/src/Enum.z42`（`Parse` / `IsDefined`）+ `tests/enum_parse_isdefined.z42` |
| 需要新建 enum 元数据来承载"底层是 i64" | ❌ **zbc 早有**：`IrClassDesc.Flags` **bit5 = enum**；`add-enum-type-metadata` 让 TYPE 记录尾部随带**成员名 + i64 值**（供反射 `IsEnum`/`GetNames`/`GetValues` + `typeof`） |
| Q3「跨包 enum 的 TSIG 表示要不要带底层类型」未知 | 🔎 已有半个答案：`ImportedSymbolLoader` 有 `EnumTypeNames` / `EnumConsts` 两张表（镜像 `SymbolTable` 同名表），imported 枚举常量本就能解析。**待测的是「跨包 enum 的类型名还原成什么」**，不是"有没有元数据" |

⇒ **design D1（`IsEnum` 标记）多半不用新造**——先查 `Z42ClassType` / `SymbolTable.EnumTypes` 与
`Flags bit5` 之间今天已经连到哪一步。阶段 0 的问题从「有没有」变成「链路断在哪一环」。

> ⚠️ 另：worktree `../z42-reflinst` 的分支 `feat/enum-underlying-type`（`fcc2be51`）**未合并进 main**
> 且提交号是 `(#28)/(#29)` 级别的陈旧物——**不要**把它当作在飞工作或参考基线。

## 阶段 0: 摸清未知（不写实现，只测）—— ✅ **已完成 2026-09-08**
- [x] 0.1 实测 `PrimModel.IsScalarValue` / `StructLayout` / `BoxIfNeeded` 今天怎么看待 enum 的
      `Z42ClassType`（design D4 标为最大未知）——**`IsScalarValue`=false；关系运算今天就红；
      `==` 今天就过；装箱退化成 `Int32`**
- [x] 0.2 实测 `ImportedSymbolLoader` 今天怎么还原**跨包** enum 类型名（Q3）——**答案：跨包 enum
      压根不走 TSIG 类型还原**，`SymbolCollector._mergeImportedEnums` 在 typecheck 前把导入 enum
      灌进 `table.EnumTypes`，与本地共用 `SymbolTable:255` 一个解析点 ⇒ **不属 R1/R3/R5 保真度族**
- [x] 0.3 实测 `Color.Red.GetType()` 折叠今天走哪条路径（`EnumTypeName`）——**折叠正确，得 `Color`**
- [x] 0.3b **定反射面边界**：`Enum.Parse`→`long`、`IsDefined(Type,long)`、`GetValues`→`long[]`
      —— 已量清现状，**取舍待 Q3 裁决**（见 proposal Open Questions）
- [x] 0.4 把 0.1–0.3b 结论写回 design.md D4 —— 已写入「阶段 0 实测结论」表

> 🔴 **阶段 0 改写了后续计划，开工前必读 design D4 的结论表**：
> ① **1.4 可删**（跨包与本地共用一个解析点，`IsEnum` 置一处即可）；
> ② **最大未知的真身是装箱**（enum 类型值装箱 → `Int32`，与 `Type.z42:104` 的 i64 承诺矛盾），
>    本变更会把更多值赶进这条退化路径，**装箱须同批修**；
> ③ **2.4 的关系比较是必做项**（今天 `a < b` 就报 E0402），`examples/patterns.z42:65` 会当场红。

## 阶段 1: enum 类型表示
- [x] 1.1 `Z42ClassType` 加 `IsEnum`（design D1 选项 B；**不加 `EnumUnderlying`**——
      `Type.z42:104` 明写 z42 一律以 i64 背书 enum，无 per-enum 底层类型）
- [x] 1.2 `SymbolTable.z42:255` enum 名解析时置标记（+ :326 嵌套 plusKey；统一走 `Z42ClassType.Enum`）
- [x] 1.3 `MemberResolver.z42:36-46` `E.Member` 的 `BoundLitInt` 类型改为 enum 类型
      （`EnumTypeName` 字段保留，`GetType()` 折叠沿用）
- [ ] ~~1.4 `ImportedSymbolLoader` 跨包 enum 还原带标记~~ —— **阶段 0 判定不需要**：
      `SymbolCollector._mergeImportedEnums:107-120` 已把导入 enum 灌进 `table.EnumTypes`，
      与本地共用 `SymbolTable:255` 解析点 ⇒ 1.2 置位即覆盖跨包。保留条目仅作留痕
- [ ] 1.5 🔴 **装箱路径**（阶段 0 新增，**未做**）：enum 类型值装箱后 `GetType()` 今天得 `Int32`。
      本变更会把更多值赶进这条路径 ⇒ 须让装箱保留 enum 身份（或至少与 `Type.z42:104` 的
      i64 承诺自洽）。**先查 `Int32` vs i64 的矛盾是不是独立既存 bug**

## 阶段 2: 转换与运算符
- [x] 2.1 `Conversion` 新增 `ConvKind.ExplicitEnum`：**不进** `ImplicitOk`、**进** `Exists()`
- [x] 2.2 `Conversion._classifyBuiltin` 加分支 B2（enum↔整数双向 + 异种 enum）
- [x] 2.3 ~~`TypeOpTyper` cast 路径~~ —— **实测无需改动**：cast 路径对任何非 None 分类都落 `BoundConvert`，ExplicitEnum 天然放行
- [x] 2.4 `TypeFacts.IsOrderable` 认 enum + **新增 `TypeChecker._checkEnumOperands` 跨操作数配对检查**（此前 `==` 完全没有配对维度，`Color.Red == 0` 一路放行）
- [x] 2.5 `PatternBinder` 关系模式 —— 复用同一个 `IsOrderable` 谓词，2.4 落地即通
- [ ] 2.6 `ExprEmitter`：enum 静态类型仍发 i64；装箱按 i64（按 0.1 结论）

## 阶段 3: 摘跳过 + 调用点/测试改写
- [x] 3.1 🔴 **已摘掉** `OverloadBinder._checkOneArg` 里 `add-argument-type-check` 留的 `_isEnumSide` 跳过
      + 删对应注释（否则 enum 位永不检查 = 留一个假保障）
- [x] 3.2 `src/tests/types/enum.z42` 全文按新语义改写（不止 4 条：补了 enum 变量/关系比较/往返 cast 的正例）
      （`Assert.Equal(404, (long)Status.NotFound)` / `(long)Direction.North == 0`）——
      **这 4 条成文断言的正是旧模型，必须改，不得绕**
- [ ] 3.3 `examples/patterns.z42:65` 关系模式确认可用
- [ ] 3.4 `z42.core/src/GC/GCHandle.z42` 按新语义确认（预期**无需改动**——两侧都是 `GCHandleType`）
- [ ] 3.5 全仓扫其余 79 处 enum 成员引用，逐处确认新语义下正确

## 阶段 4: 验证与归档
- [ ] 4.1 新增负例/正例测试（spec 场景逐条），🔴 用 `DumpBody`/`collectDiags` 断言，
      **不得只用 `SemanticDump.FirstErrorCode`**（不合并 collector 诊断 → 空门）
- [ ] 4.2 🔴 **反向自检**：整体退回改动，新增负例必须全红
- [ ] 4.3 完整 `xtask test` 全绿
- [ ] 4.4 🔴 **自举字节不动点**：gen1 == gen2 —— 实证 design D1 的「签名键不变」，不得靠推理
- [ ] 4.5 `xtask test stdlib --mode jit`（本地 `xtask test` 只跑 interp；enum 表示相关须验 JIT 面）
- [ ] 4.6 `xtask test bootstrap`（无新语法/无格式改动，上一 nightly 应照常编过）
- [ ] 4.7 文档：**改写** `docs/book/src/runtime/struct-value-semantics.md:232` 的 enum-as-int SoT 段；
      新建 `docs/book/src/language/enums.md` 并挂进 `SUMMARY.md`
- [ ] 4.8 归档 + 随 PR 一起提交

## 备注

- **本变更会让 `Color c = Color.Blue;` 第一次能编过**——那是今天就编不过的既存 bug（欠债表 bug D），
  不是新功能。
- 🔴 **禁止**用「在调用点补 cast」把红变绿来替代本变更（那是拿 cast 掩盖编译器 bug）。


## 🔴 实施期校正（2026-09-08）—— 阶段 0 有一条结论是错的

**阶段 0 断言「跨包 enum 与本地共用 `SymbolTable:255` 一个解析点、不属 imported 保真度缺口」——
这条被实施期的数据推翻了，已在代码注释与 design 里更正。**

真相是**两条独立路径**，当时只量了一条：

| 路径 | 谁走 | 阶段 0 是否量到 |
|---|---|---|
| 源码里**写出的**类型引用（`GCHandleType t = …`） | `SymbolTable.EnumTypes` → `Z42ClassType.Enum` | ✅ 量到了 |
| **导入成员签名里**的类型（`Type.Visibility` 的返回类型） | `ImportedSymbolLoader._resolve` | ❌ **漏了** |

`_resolve` 从不查 `EnumTypeNames` ⇒ 导入签名里的 enum 落到末尾 prim fallback、丢标志 ⇒
`t.Visibility == TypeVisibility.Public` 报「enum 比非 enum」。**enum 确实属于 R1/R3/R5 那族
imported 类型保真度缺口**，已按根因在 `_resolve` 补 `EnumTypeNames` 分支（放在 `r.Classes` 之前）。

暴露它的是**跨文件扫描**（`type_visibility.z42` / `gc_handle.z42` / `heap_retention.z42` 同时红），
不是单点探针——教训：**「共用一个解析点」这类全称结论，必须把所有入口都枚举一遍再下**。

## 🔴 验证方法论：GREEN gate 验不了本变更的负例面

`xtask test e2e --dir types` 在 enum.z42 **还写着旧断言时就全过 126/126**——因为 golden 走
`--emit-zbc`，那条路**吞诊断**（正是 `restore-emit-zbc-diagnostics` 在修的洞）：新增的编译错误
不可见，而 emitter 照发的代码碰巧还能跑。

⇒ **本变更的负例面必须用 `z42c build` 路径单独扫**（`scratch/scan/` 合成工程逐文件编）。
扫描配方与两个必踩的坑：
- **每次迭代前清 `.cache` + `dist`**，否则 cached 文件的诊断不打印 → 假绿；
- **退回对照的基线必须真的能跑**：我第一次的「修前零错误」是假的——`scratch/sdk/bin/z42vm`
  没有执行权限、`programs/z42c/` 已空，而 `| tail` 把退出码吞成 0。**永远先验基线自身 exit 码**。
