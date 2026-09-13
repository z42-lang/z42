# Proposal: 裸构造器键在 skew 下会命中**错的**构造器 —— ObjNew 加 arity 校验

> **状态：IMPL**（本 PR 只做站点 ⑤）| 创建：2026-09-13 | 前置：`fix-silent-symbol-resolution`（PR #614）已合并
> 归属程序：依赖 zpkg 版本 skew（`available!` 主动降级 + 缺符号用到才抛）

## Why

PR #614 把站点 ③（`ObjNew` 的构造器解析不到）改成抛 `MissingSymbolException`，但**只在
`argc > 0` 时**——「没有构造器的类」与「单构造器的类」发出的 ctor 键**同形**，运行期无法
区分。当时把 `argc == 0` 记为已知残留。

复核这条残留时查出两件事，第一件推翻了原定补法，第二件是**没人覆盖、且危害更大**的形态。

### 事实一：原定补法（TypeDesc 记录本类构造器）几乎没有覆盖面

`_ctorKey`（`OverloadBinder.z42:356`）在类无构造器时回落**裸类名**；而
`stabilize-instance-dispatch-keys` 规定 **primary 构造器必用裸键**。两者相乘：

> **类只要声明了任何构造器，裸键就一定解析得到。**

所以「裸键解析不到 + argc==0」时，被加载的那份类几乎必然真的零构造器 —— 新元数据永远判
不出问题。它唯一能抓的是极窄一条缝（0-arg 构造器恰是**非**-primary、旧版没有、旧版还有别
的构造器）。**不值得为它动 `TypeDescCold`。**

### 事实二：同一个裸键会在 skew 下**解析到错的构造器**（本 change 的目标）

| | 编译时依赖（v2） | 运行时加载到（v1） |
|---|---|---|
| 声明 | `class Widget { Widget() {...} }` | `class Widget { Widget(int v) {...} }` |
| 裸键 | `Demo.T.Widget.Widget`（primary） | `Demo.T.Widget.Widget`（primary） |

`new Widget()` 发裸键、argc=0 → 运行期**解析成功**，命中 `Widget(int)`，`exec_function` 用
`Frame::new(args, max_reg)` 建帧、**不做任何 arity 校验**（`interp/exec_support.rs:27`），
形参 `v` 停在默认值上继续跑。**实测修复前打印 `constructed 0`。**

这不是「缺符号」，是**静默调错构造器**——正是本程序要根除的那类静默错答案，却整个漏在网
外；而且比 `argc == 0` 那条缝常见得多：**任何**「构造器签名变了」的 skew 都落在这里。

## What Changes（站点 ⑤）

`ObjNew` 解析到构造器**之后**，校验它容不容得下实参数。判据全部来自**已有**元数据，
无格式改动：

```
phys = argc + 1                       // 调用方实际传入的值个数（含 this）
min  = min(min_arg + 1, param_count)  // ⚠️ min_arg 两种口径并存，必须夹住
max  = params_from != 0xFF ? ∞ : param_count
phys ∉ [min, max]  ⇒  抛 MissingSymbolException
```

- **`min_arg` 的夹取**：文档口径是逻辑必填数（不含 `this`），`_fillParamMeta` 写的也是逻辑
  值；但 `IrFunction` 构造器的**默认值**是 `MinArg = paramCount`（**物理**总数，含 this）。
  没被 `_fillParamMeta` 覆盖的合成函数会多算 1，不夹住就假阳性。夹到 `param_count` 后默认
  情形退化成「全必填」——正是该默认值本来的语义。
- **复用 `MissingSymbolException`**：新异常类要先进 stdlib，冷启动种子里没有它 ⇒ 得走
  两-nightly。语义上也说得通：调用点指名的那个重载**确实不在**。
- **两后端都查**，且 JIT 的 native 分支不漏：跨包构造器正是惰性加载、最容易 tier 到 native
  的那批。区间在 `FnEntry` 里随编译一次算好，两条分支都无额外查表。
- **顺带**：`ObjNewInstr.Dump()` 此前**根本不打印 ctor 键** —— 这正是「这条 obj_new 会调哪个
  构造器、还是压根不调」在所有 IR 断言里都看不见的原因。补上，并加三条 codegen 断言，其中
  一条把「零构造器与单构造器发同一裸键」这个歧义**钉住**：谁解开了歧义，它就会红。

## Out of Scope（都已从本 PR 拆出，User 2026-09-13 裁决）

- **给「零构造器」一个可区分的编码**（原计划的阶段 ①②）：另立 change。判据算不准是卡点
  ——z42c 为「有字段初始化器、无显式 ctor」的类**合成**的隐式构造器既不是 `MethodSymbol`、
  也不在 `_bindNew` 那一刻存在（`_synthCtors` 跑在所有绑定**之后**），而准确的 oracle
  （本包全部已发射函数 ∪ `DependencyIndex.Statics`）要等整包装配后才齐。判错 = **静默跳过
  真构造器**，比它要修的 bug 更坏。第一次尝试用符号表判据，实测让 4 个 stdlib 用例变红。
- **跨包构造器默认值尾参塌零**（本轮由对照组 fixture 查出的第三个独立 bug）：`_bindNew` 手
  写了一套实参适配、**漏了跨包默认值那一支**（方法路径一直有 `_withDefaults` 的
  `_crossPkgDefault`）。补丁已验证可行，另开 PR。
- 通用调用（`Call`/`VCall`）的 arity 校验、包级版本元数据、急切 `--verify-links`：未立项。

## 验证

| 门 | 结果 |
|----|------|
| `wrong_ctor_arity_skew`（新）| 修复前 `constructed 0` **FAIL** → 修复后 **PASS** |
| `wrong_ctor_arity_present`（新，配对在场对照）| 无构造器类 / `params` 构造器 / 跨包合成构造器三种合法形态**都不得误报** |
| `symres_tests.rs`（新，5 条）| `min_arg` 口径夹取等；实测去掉夹取后精确红一条 |
| 全量 GREEN | `xtask test` 全绿 |
| JIT | `xtask test stdlib --mode jit` |
