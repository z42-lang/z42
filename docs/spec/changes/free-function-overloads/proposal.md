# free-function-overloads — 自由函数支持重载

> 状态：**IMPLEMENTED（阶段 1 = support 先行）**（User 确认方向 2026-09-22 → IMPL → GREEN）
> 来源：教程读者提问「自由函数为什么不能重载，能支持吗」
> 落地：编译器 14 文件 + 单测 3 条 + e2e 2 fixture（basic + cross-zpkg）+ 文档（learn/reference/internals/examples）。
> 阶段 2（stdlib/z42c 源码真正 use 重载）另起，须晚一个 nightly。

## 问题

z42 的**顶层自由函数**今天不支持重载：同名即报 E0408，与参数类型无关
（[functions.md:51](../../../learn/src/basics/functions.md)、[DeclBinder.z42:41-58]、
[MemberCollector.z42:36-46]）。而**类方法**（实例/静态）本就支持重载。这个不对称不是语言层面刻意
禁止「重载」这个概念，而是自由函数的**符号表结构 + 派发键**的历史限制：

- 自由函数按 `ns.name` **裸名唯一键**登记进 `SymbolTable.FunctionsByFqn`
  （[SymbolTable.Functions.z42:16-31]），键里**没有 arity / 形参类型** ⇒ 一个 `ns.name` 只能存一份。
- 调用点是**单符号查表**（`ResolveFuncNs` + `GetFuncIn`，[MemberResolver.z42:682-691]），
  不经 `OverloadResolver` 的候选集/适用性/最具体决议——这条路径类方法才走。
- 类方法的重载 mangle 键（`Name$arity$T...`）已经进了 zbc 符号串
  （[OverloadResolver.MangleKey]、[MemberCollector._fillClass] 的 primary/非-primary 分支），
  自由函数从没接上。

一句话：自由函数「不重载」是**实现结构限制**，不是设计禁令。补齐它 = 把自由函数接到类方法**已有**的
那套重载机制上。

## 为什么可行、且成本可控（关键判断）

朴素做法「把自由函数符号从 `ns.name` 全量改成 `ns.name$arity$T`」会 **re-mangle 所有存量自由函数
符号** ⇒ 打断自举不动点 ⇒ 需格式 bump + 两代自举（roadmap.md:400 那堵墙）。

**但这堵墙已被 #414（stabilize-instance-dispatch-keys，方案 A）拆过**：实例/静态虚方法采用
**primary（声明序首个同名）保留裸键 / 非-primary 才带全签名后缀** 的键规则，于是「给已有名字加一个
重载」是**纯增量**——存量符号一个字节都不动（[MemberCollector.z42:148-151, 282-298]）。

把**同一套 primary-bare scheme 套到自由函数**，得到关键性质：

- 今天每个名字只有一个自由函数 ⇒ 它是 primary ⇒ 键仍是裸 `ns.name` ⇒ **存量 zbc/zpkg 字节不变**。
- 加第二个同名重载时，primary 保裸名、新来者才拿 `$arity$T` 后缀 ⇒ 纯增量，不 re-mangle 任何已有符号。

运行期（Rust VM）**零改动**：VM 把函数名当不透明字符串，`func_index` 是 `String→usize`、全仓不 parse
`$`（[indices.rs:69]、[exec_call.rs:125]）；静态方法调用今天走的就是这条同表同键路径，mangle 串天然可用。
唯一需实测确认的是入口点 `Main`（见 T7）。

## 设计

### D1 — 键规则：复用 #414 primary-bare（逐字对齐）

自由函数 `MethodSymbol` / `MethodDecl` 获得 `RegKey`，按**每 ns 的声明序** tracker 决议：

- 某 `(ns, name)` **首个**声明 → primary → `RegKey = name`（裸；`QualOf(ns, name)` = 旧键，零漂移）。
- 后续同名 → 非-primary → `RegKey = MangleKey(name, paramTypes, paramCount)`
  （`QualOf(ns, RegKey)` = `ns.name$arity$T...`）。

tracker 与类方法的 `emittedInst` 同构，但**按 ns 分桶**（不同 ns 各自从 primary 重开）。复用
`OverloadResolver.MangleKey`，不新造键派生。

### D2 — 候选集模型：SymbolTable.Functions

`FunctionsByFqn`（`ns.name → 单符号`）扩为「能按 `(ns, name)` 枚举所有重载」。方案：

- 新增伴生索引 `FuncOverloadsByFqn`：`ns.name`（**基名 FQN**）→ `MethodSymbol[]`（该 ns 下同名全部重载，
  含 primary）。`AddFunc` 追加。
- `FunctionsByFqn` 保留、语义收敛为「基名 FQN → **primary** 符号」（对唯一函数即那一份，
  所有既有 `GetFuncIn` 单符号消费点行为不变——它们要的就是 primary/唯一那份的签名与身份）。
- 新增 `GetFuncCandidates(ns, name) → MethodSymbol[]` 供调用点做重载决议。

> 决策点（写进 tasks，实现时定稿）：是否让 `FunctionsByFqn` 直接存候选数组、`GetFuncIn` 返回 primary，
> 而非并存两张表。以「既有单符号消费点改动最小 + 字节稳定」为准绳。

### D3 — 调用点接入重载决议

[MemberResolver.z42:682-691]（裸名 `f(args)`）与 [:747-749]（`ns.f(args)`）：`GetFuncIn` 取单符号 →
改为 `GetFuncCandidates` 取候选集，走 `OverloadResolver.Resolve` / `ResolveMapped`（与类方法同一决议，
含默认值/命名实参/params）。选中符号的 `RegKey` 写进 `BoundCall.MethodName`（对齐静态调用
[:703] 用 `sms.RegKey`）；`FreeNs` 不变。no-match / 歧义走 `OverloadResolver` 结果分派诊断
（自由函数此前连 no-match 诊断都没有，roadmap.md:384，顺带补上）。

### D4 — 重复检测收紧（E0408）

[DeclBinder._checkDuplicateFreeFunctions]（[:41-58]）与 [MemberCollector.z42:41-46] 的「同名即 E0408」
改为**仅同签名才报重复**——判重键用**全签名 MangleKey**（与 [DeclBinder.z42:60-64] 类方法判重同款，
Canon 归一 nullable/别名，不随 primary 选取漂移）。跨文件同 FQN 且**同签名** → 仍 E0408 并指双方；
不同签名 → 合法重载。

### D5 — 发射（def 侧 + call 侧 + 取引用）

- def 侧 [IrGenAuxEmitter.EmitFreeFunctions:103]：`_q(md.Name)` → `_q(md.RegKey)`（primary 裸 → 同名，
  字节稳定）；体查找键同步。
- call 侧 [CallEmitter.z42]：已用 `QualOf(c.FreeNs, c.MethodName)`；D3 令 `MethodName = RegKey` 后自动带签名。
- 取引用 [ExprTyper.z42:101-104] `BoundFuncRef` + [ExprEmitter.z42:194] `LoadFn`：方法组指向自由函数时，
  **v1 只在无歧义（该名恰一份）时可取引用**；同名多重载取引用 = 需 target-type 定向，暂报诊断
  （与类方法方法组取引用的现状对齐；避免 v1 引入方法组重载消解的复杂度）。

### D6 — 跨包导出/导入

- 导出（TSIG）：自由函数按 `RegKey` 导出（primary 裸 → 与今天逐字相同；重载才多出 `$` 条目）。
  须核 `ExportedTypeExtractor._extractFunc`（按名取符号）改为遍历候选。
- 导入 [ImportedSymbolLoader.z42:351-367] + [SymbolCollector.z42:230-236]：`FunctionsByFqn` 键已是
  RegKey-qualified FQN；loader/merge 改为按候选集并入（一名多份），不再 first-wins。

### D7 — 入口点 Main（实测项）

运行期入口按 `f.name == entry_name` 精确匹配（[vm.rs:90-99]）。`Main` 天然是 primary → 裸键
`Main`，`z42c build` 烘焙的 entry 串不变 ⇒ **预期零影响**。**必须实测**：`z42c build` 一个含 `Main` 的
程序，跑通即验证。

## 分阶段引入（bootstrap-seed.md 铁律）

- **阶段 1（本变更，support 先行）**：z42c 获得「解析 + 发射自由函数重载」能力；
  **stdlib / z42c 自身源 / xtask 不写任何自由函数重载**。primary-bare 保证对现有代码零发射字节变化。
- **阶段 2（晚一个 nightly，use）**：确认阶段 1 已随 nightly 发布后，才允许在种子消费代码里真正写重载。

## 验收门（GREEN + 字节对账）

1. `xtask test`（interp）+ `test stdlib --mode jit` 全绿（改了派发面，见 local-green-misses-jit）。
2. **字节对账**：阶段 1 前后，对现有语料（stdlib + z42c 自身）编出的 zbc/zpkg **逐字节相同**——
   这是「零 bump / 零两代自举」的硬证据，不达成则必须查 primary-bare 不变量的泄漏点。
3. 新增正例：不同签名自由函数重载能编译、能按实参正确派发（interp + jit）。
4. 新增负例：同签名重复仍报 E0408（指双方）；重载 no-match / 歧义有清晰诊断。
5. 冷启动自建（`compile-toolchain`）过——本变更不新增跨层符号，风险低，但仍需 CI 确认。

## 非目标（v1 明确不做）

- 方法组/`BoundFuncRef` 对**同名多重载**自由函数取引用的 target-type 消解（D5，暂诊断）。
- 阶段 2 的 stdlib 实际使用重载（另起变更，晚一个 nightly）。
- impl-block 方法并入 primary/非-primary（[SymbolCollector.z42:384-385] 明确另议）。
