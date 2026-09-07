# Tasks: 调用实参类型检查

> 状态：🟢 已完成 | 完成：2026-09-07
>
> 归属程序：[[restore-emit-zbc-diagnostics-program]] 第 ⑤ 步。
> **未获 6.5 确认前不得写实现代码**（lang 类变更，见 workflow.md Spec-First Self-Check）。
>
> **裁决**：Q1 = **一个 PR 全做**（阶段 1–4 同一交付）；Q2 = enum ↔ 整数**要求显式 cast**；
> Q3 = **构造器同批接**。

## 进度概览
- [x] 阶段 0: 探索与量测（见 proposal 爆炸半径表）
- [x] 阶段 1: R1 型参身份修复链 —— **实为两半**（R1a 身份 + R1b 递归擦除，见 design 校正）
- [x] 阶段 2: R2 / R7 —— 赋值上下文已可复现的既存 bug
- [x] 阶段 3: R3（消费端半边）/ R5（真根因是 imported 委托退化）/ R6（→ 跳过，拆独立 change）
- [x] 阶段 4: 接线开检查 + 构造器 + 负例门（22 条）
- [x] 阶段 5: 验证与归档

## 实施期校正（三处，均以实测为准）

1. **跨包路径是 `ZpkgReader` 不是 `ZbcReader`** —— 初稿的修复链写错了 reader，已回退重做。
2. **R1 是两半**：只修型参身份后，跨包**裸 `T`** 转绿、**`T[]` 全数仍红** ⇒ 还缺递归擦除。
3. **R5 的真根因是 imported 委托类型退化**（`Action` → 普通类），不是「lambda 推断」——
   接上目标定型后 `Thread.Start(() => {…})` 依旧红，直到补了 `_resolveDelegate`。

⇒ R1/R3/R5/R7 四条同属一族：**`ImportedSymbolLoader` 的类型保真度**。

> **Q1 = 单 PR**：阶段 1–4 一次交付，任何提交点上树都自洽。
> ⚠️ **rebase 纪律**（单 PR 的代价）：main 被并发会话高频推进，每次 rebase 要重跑 ~10 分钟 GREEN
> → **实施期间不中途 rebase**，只在**合并前一次性** `git fetch && git rebase origin/main` + 完整 GREEN。

## 阶段 0: 探索与量测 ✅（本轮已做，无需重做）
- [x] 0.1 复现「实参零检查」：`TakeS(42)` rc=0、产物照写、运行期不报错
- [x] 0.2 定位三条调用路径的缺口（自由函数连 arity 都不查）
- [x] 0.3 确认可复用机制：`CheckImplicitConvert` + `Conversion.Classify(...).ImplicitOk()`
- [x] 0.4 确认汇聚点：`FillDeferredArgs`（21 处调用；`sig!=null` ⟺ 签名已知）
- [x] 0.5 探针实编全仓（**cache-cold**）：stdlib 39 / z42c 1 / 语料 54（14 文件为本检查独有）
- [x] 0.6 六个根因逐条溯源，确认**零真实用户类型错误**
- [x] 0.7 核实 R1 无需格式 bump（tp 块已含型参名，现读弃）

## 阶段 1: R1 —— 跨包型参身份 + 递归擦除（79 条 / 84%）
- [x] 1.1 ~~`ZbcReader`~~ → **`ZpkgReader.z42:217-224`** 捕获 tp 块型参名（此前 `c.U32()` 读出即丢）。
      🔧 初稿写错 reader：`ImportedSymbolLoader` 的输入来自 `DepScan → TsigReconcile.Rebuild(ZpkgInfo)`，
      不是 zbc SIGS 路径。（附带发现：`SigEntryZ.TypeParamCount` 全仓无消费方，是独立死字段。）
- [x] 1.2 `ExportedTypes.z42` `ExportedMethodZ` 增 `TypeParams[]`（**ctor 元数不变**，构造后赋值——
      旧种子 ABI 约定，同 `ParamsFrom`/`IsSealed`/`TypeParamCount`）
- [x] 1.3 `TsigReconcile._methodFromSig` 把型参名传到 `ExportedMethodZ`
- [x] 1.4/1.5 `ImportedSymbolLoader._tpsWith` 合并类级 ∪ 方法级型参，接**三处**站点：
      类方法、**接口方法**（此前一个都没喂 → `IBasicCollection<int>.AddOne(1)` 的 `int→T`）、trait-impl
- [x] 1.6 **R1b（实施期新发现）**：`Conversion._hasGenericParam` 把擦除判定改为**结构化递归**。
      只修身份不够——对照实验：跨包裸 `T` 转绿、`T[]` 全数仍红（两侧同为 `Z42ArrayType`，顶层判定恒相等）
- [x] 1.7 **自举字节对账**：型参身份改动**实测未动签名键**（3/3 gen1==gen2）——不是推理

## 阶段 2: R2 / R7 —— 赋值上下文已可复现的既存 bug
- [x] 2.1 R2：`Conversion._classifyBuiltin` 加分支 C2「任意数组 → `Array`」→ `ImplicitRef`
- [x] 2.2 R2 回归：`Array a = new int[3];`（var-decl）+ `TakeArr(new int[3])`（argument）
- [x] 2.3 R7：`Z42FuncType.IsAssignableTo` 改**逐位结构比较**（arity + 各形参 + 返回，叶子按 `CanonName`，
      吸收 unknown/error），并加「imported 委托退化成同名 `Z42ClassType`」的双路桥接
- [x] 2.4 R7 回归：`Func<int>` ↔ `Func<Int32>`（单测锁住）

## 阶段 3: R3 / R5 / R6
- [x] 3.1 R3（**消费端半边**）：`_resolve` 把 `"unknown"` 哨兵还原为 `Z42UnknownType`（此前物化成
      「名叫 unknown 的类」→ Absorb 守卫失效）。🕳 产出端半边（让 SIGS 写真实限定名）会改 SIGS 字节 +
      有反射运行期后果 → **留作独立 change（bug B3）**
- [x] 3.2 R5：lambda 实参纳入 target-typed 延迟绑定通道 + `BindWithTarget` 加 lambda 目标分支
- [x] 3.3 R5 **真根因（实施期修正）**：不是「lambda 推断」，是 **imported 委托类型退化**——
      `Action`/`Func<…>` 经 `ImportedSymbolLoader` 成了普通类 ⇒ `target is Z42FuncType` 不成立。
      补 `_resolveDelegate`（对齐本地 `SymbolTable.ResolveTypeP:238-254`）后才真绿
- [x] 3.4 R5 副作用确认：lambda IR 类型由 `unknown` 变真实类型 → **发射字节确实变了**（见 5.x）；
      golden 无需 regen（全绿），自举不动点已重新确认收敛
- [x] 3.5–3.7 R6 → **不在本变更**：改为跳过 enum 位 + 登记残留洞，语义拆去
      `make-enum-distinct-type`（另一分支）（理由见 design D6）

## 阶段 4: 接线开检查 + 构造器 + 负例门
- [x] 4.1 `FillDeferredArgs` → `BindArgsToSignature`（该方法现在既回填延迟位又按签名校验）
- [x] 4.2 逐位检查：`sig!=null` 时调 `CheckImplicitConvert(arg, pt, syms, rawArgs[i].Span, "argument")`
- [x] 4.3 `params` 尾位按元素类型逐位检查；定长段 `[0, ParamsFrom)`
- [x] 4.4 检查在 `BoxArgs` / `_withParamsExpansion` **之前**
- [x] 4.5 **不短路**：同一调用多个不符实参逐条报（单测锁住）
- [x] 4.6 构造器（Q3）：`ConstructTyper` 复用**同一个** `CheckArgTypes`（非逐位 `CheckArg`）——
      ⚠️ 接线时实测踩到回归：`P(params int[] xs)` 接 `new P(1,2,3)` 逐位比会误报，
      `typecheck_tests.test_ctor_params_variadic_ok` 当场变红 → 已加同款正例锁住
- [x] 4.6b `_adaptArgs` 的命名实参 / 可选参路径就地 `CheckArg`（那条路不经汇聚点）
- [x] 4.7 新增 `tests/typecheck/argument_type/argument_type_tests.z42`（**24 条**）
- [x] 4.8 断言用 `bodyDiags` 读整袋（**码 + 条数**），不只看 `FirstErrorCode`
- [x] 4.9 反向自检 —— 见下「备注」的两次尝试（第一次是无效实验）

## 阶段 5: 验证与归档
- [x] 5.1 `cargo build --release` —— z42vm（GREEN 内含）
- [x] 5.2 完整 `xtask test` 全绿
- [x] 5.3 自举不动点 3/3 gen1==gen2（含冷种子按 CI 顺序复现，见 5.x）
- [x] 5.4 无新语法 / 无格式改动 —— zbc·zpkg writer 未动，不触发 bootstrap-seed 的两-nightly 纪律
- [x] 5.5 `xtask test stdlib --mode jit` —— ✅ **332 文件 / 23 库全过**（R5 改了 lambda 类型标注，
      本地 `xtask test` 只跑 interp，故必验 JIT 面，见 [[local-green-misses-jit-and-lines]]）
- [x] 5.6 文档同步：`docs/book/src/compiler/source-compile.md` 增「调用实参类型检查（与赋值同一条门）」节
      —— 汇聚点 + `sig!=null ⟺ 签名已知` 不变量 + imported 保真度三降级表 + **残留洞四条**
- [x] 5.7 `source-compile.md` 页头「对齐」刷新为 2026-09-07
- [x] 5.7b `docs/roadmap.md` Deferred Backlog Index 加两行（重载 no-match 诊断 / enum 独立类型）
- [x] 5.8 归档 `changes/` → `archive/2026-09-07-add-argument-type-check/`，tasks 标 🟢，随 PR 一起提交
- [x] 5.9 更新 memory `restore-emit-zbc-diagnostics-program.md`：⑤ 完成、欠债分母重算

### 5.x 自举不动点：一次性代数差（已查清，非缺陷）

**R5 让 lambda 类型从 `unknown` 变成真实类型 ⇒ 发射字节确实改变**（spec 的 IR Mapping 节已预判）。
字节级定位：gen1/gen2 差 12218 字节，首个差异在偏移 67784，gen1 是字符串 **`<unknown>`**。

| 场景 | 结果 |
|---|---|
| 冷种子后**裸跑** `xtask test`（我的非常规调用） | ❌ 1/3 —— gen1 由种子（旧发射器）编出 |
| 再跑一次（in-tree 已是新发射器） | ✅ 3/3 收敛 |
| **照 CI 顺序**（`build compiler` → `build stdlib` → `test compiler`），冷种子 | ✅ **3/3** |

⇒ 新编译器**自身是稳定不动点**；CI 的构建顺序不受影响。**不需要格式 bump、不需要两代自举**。

## 备注

- **本变更不开 P1 的门**（`--emit-zbc` 打印诊断）——那是本程序第 ⑧ 步，仍排最后。
  但量测阶段临时套用了 P1 补丁（`../z42-diagdebt`）才能看见单文件路径的诊断。
- **量测陷阱（务必记住）**：`z42c build` 跳过 cached 文件 → 不打印其诊断。任何"扫全仓数欠债"
  的动作**必须先清 `.cache` / `dist`**，否则得到的是低估（本轮首次扫描 37 → 冷跑 39，且 25 个包
  曾整包 cached）。
- **再次目击**：`xtask build compiler` 在 member / bootstrap 失败时仍打印 `✔` 并 exit 0
  （本轮 2 次）。memory 已记，值得单独立项。判读构建结果**必须 grep `✗`**，不能信 `✔` / exit code。
- 🔴 **反向自检的第一次尝试是无效实验，值得记**：退回**根因修复**（保留检查）后 `test compiler`
  报「0 失败」——看着像自检通过，实则 **stdlib / z42.core 根本没编过、测试一次都没跑**
  （`ArgumentTypeTests` 在日志里出现 0 次）。差点把假信号当成结论。
  → **判据**：先确认「用例真的跑了」（grep 用例名计数），再看红绿。
  → 有效实验改成**只关检查、保留全部根因修复**（树仍可编）→ 7 条负例全红 ✅。
  → 顺带得到一条更强的证据：**检查开着 + 任何一条根因缺失 ⇒ 整棵树编不过**，说明六条根因全是承重的。
