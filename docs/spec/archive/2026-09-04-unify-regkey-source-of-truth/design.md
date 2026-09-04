# Design: 方法注册键推导的单一 owner

> 配套 [proposal.md](proposal.md)。行号基于 `origin/main @ 44e67ef1`。

## 1. 事实基线（全部经代码核实）

### 1.1 字段与默认值

| 项 | 位置 | 值 |
|---|---|---|
| `MethodDecl.RegKey` 声明 | `z42c.syntax/src/Decl.z42:140` | `public string RegKey;`，注释写「`""` = 未填（回落旧逻辑）」 |
| 默认值 | `Decl.z42:152` | 构造函数**显式** `this.RegKey = "";` |
| `MethodSymbol.RegKey` 默认 | `Symbol.z42:32` | **`= name`**（裸名，非空串）|

> 这处**非对称**（符号侧默认裸名、AST 侧默认空串）本身就是混淆源之一。

### 1.2 赋值点穷举

| 位置 | 目标 | 覆盖 |
|---|---|---|
| `MemberCollector.z42:220` | `md.RegKey` | **唯一**给 AST 写的地方；仅 class/struct 的 `c.Members`，且被 `:206` 擦除分支排除 |
| `InheritanceResolver.z42:120-121` | `localSym.RegKey` + `md.RegKey` | override 槽对齐**改写**；前置 `ct.Methods.Get(md.RegKey)` 非空（`:115`）→ 空串时 `Get("")` 必 null → **不会把空串变非空** |
| `ImportedSymbolLoader.z42:303` | `sym.RegKey` | imported 符号，**只写符号不写 AST** |

### 1.3 恒空的两类（可达消费点）

**(A) impl-block 方法**：`_passMembers`（`:17-28`）不分派 `ImplDecl`；
`InheritanceResolver._passImpls`（`:33`）`target.Methods.Put(md.Name, …)` 用裸名、不写 RegKey。
用例：`src/tests/generics/extern_impl_user_class.z42`、`src/tests/cross-zpkg/impl_propagation/`、
`src/tests/cross-zpkg/impl_reflect/`。

**(B) decl-only partial method**：`MemberCollector.z42:205-208` 擦除分支。
用例：`src/tests/partial-types/partial_method.z42`。

> **关键细节（决定 §3 的实现）**：擦除分支执行时，`regName` **已在 `:195` 算好且恒为 `md.Name`**
> —— 因为 `tracks = !(md.IsPartial && !md.HasBody)`（`:196`）为 false，`:197-203` 的
> primary/非-primary 决议整块跳过。**不需要复算任何东西。**

### 1.4 恒空但**不可达**这些消费点的三类（无需处理）

| 类别 | 为何不可达 |
|---|---|
| 接口方法 | `_fillInterface`（`:39-41`）裸名注册；`_bindClass` 只对 class/struct 调（`TypeChecker.z42:312`）；导出走独立 `ClassExtractor._extractInterface`（`:12-31`）|
| 顶层自由函数 | 走 `DeclBinder._bindFreeFunc`（`:354`）与 `_extractFunc` |
| 合成 `MethodDecl` | `IrGenMemberEmitter:69/82/117/135`、`StubEmitter:166`、`AttributeSynth:139`、`BenchmarkDesugar:82/112` 等 —— 全部**直接**交 `FunctionEmitter.EmitFunction` 并显式给 IR 名，不进 `EmitMethod` |

## 2. 消费点现状与收敛后形态

| # | 位置 | 现依赖哪一档 | 收敛后 |
|---|---|---|---|
| 1 | `ClassExtractor.z42:194-196` | **③**：跨-CU partial 时 `classMap` per-CU last-wins（`ExportedTypeExtractor.z42:111-120`），decl 碎片所在 CU 靠回落才能把方法写进该 module 的 TSIG | `MethodKeyOf(chain[mci].Methods, amd)` |
| 2 | `ClassExtractor.z42:328-330` | 无 `HasBody` 卫兵 → `static partial` decl-only 可达；**全仓库无此写法** | `MethodKeyOf(ct.Methods, smd)` |
| 3 | `DeclBinder.z42:188-190`（`_bindImpl`）| **完全依赖 ③**：impl 方法 RegKey 恒空 → `"Hello$0"` miss → 回落 `"Hello"` 命中 `_passImpls` 裸键 | `MethodKeyOf(ct.Methods, md)`；写侧补齐后 ② 直接命中 |
| 4 | `DeclBinder.z42:237-239` | **已是死代码**：`if (md.HasBody)` 卫兵（`:234`）排除 decl-only partial；且 `:245` 无条件解引用 `ms`（`md.IsCtor && !ms.IsStatic`）——真有漏网早该 NPE，自举常绿即反证 | 同上 |
| 5 | `DeclBinder.z42:336-338`（`_checkExposure`）| 无 `HasBody` 卫兵 → decl-only partial 可达 | 同上；⚠️ 移除回落时 E0441 **重复条数**会变，需对账 |
| 6 | `IrGenTypeEmitter.z42:122-124`（`EmitImpl`）| 与 #3 **成对**：#3 产 body 键、#6 消费同键拼 IR 名 `g._qClass(itn) + "." + imk`（`:129`）| `MethodKeyOf(iowner.Methods, imd)`（含 `iowner == null` 处理）|
| 7 | `IrGenMemberEmitter.z42:18-20` | 键在 `:18-20` 算、`HasBody` 到 `:22` 才判；但后续三分支（`HasBody`/`extern`/`abstract && !static`）对 decl-only partial **全不成立** → 算出的 `irName` 无人用 | **不查表**，单独处理（见 §3.3）|

> **单独排除**：`CallEmitter.z42:238-243` 的静态 DepIndex 查找。其
> `arityKey = MethodName + "$" + ArgCount` 与 `DependencyIndex.AddModule`
> （`z42.ir/src/DependencyIndex.z42:88-123`）注册的四种键
> （`Cls.<全串>` / `Cls.<bare>` / `ns.Cls.<全串>` / `ns.Cls.<bare>`）**都不匹配** → 当前是死路
> （真正命中的是 `:241/:243` 的全键探测）。但跨版本自举时旧 nightly 产物可能仍是 `Name$arity`
> 形态，且该段注释反复强调「字节不动」→ **不在本变更范围**。
>
> 另需修正一个常见误解：静态方法**并非**恒全 mangle ——
> `MemberCollector.z42:177` 的 `staticVirtual = mst && (override || abstract)` 走基线裸键
> （INumber 的 `op_*` 等），理由见 `:170-173`（VCall 按运行时类型派发，base 与派生必须同键）。

## 3. 实现

### 3.1 写侧：单一注册入口

现有两个注册点，键的算法各不相同却都要写字段：

| 注册点 | 键 | 现状 |
|---|---|---|
| `MemberCollector._fillClass`（`:219-221`）| primary 裸 / 非-primary `MangleKey` | 写了 `msym.RegKey` + `md.RegKey` |
| `InheritanceResolver._passImpls`（`:33`）| **恒裸 `md.Name`**（impl 不支持同名重载）| **两个都没写** |

收敛为：

```z42
// 唯一注册动作：算键与写键绑死，结构上不可能只做一半。
// impl-block 方法恒用裸名 —— 它不参与 primary/非-primary 规则；
// 把它并入是独立的语义扩展（会 rekey、需格式 bump），本变更明确不做。
private void _registerMethod(Z42ClassType ct, MethodDecl md, MethodSymbol sym, string key) {
    sym.RegKey = key;
    md.RegKey  = key;
    ct.Methods.Put(key, sym);
}
```

- `_fillClass` 传入算好的 `regName`
- `_passImpls` 传入 `md.Name`

**字节中性**：`_fillClass` 行为逐字不变；`_passImpls` 新写入的 `md.Name`
**恰是消费点 ③ 今天算出的值**。

### 3.2 读侧：单一解析 helper

```z42
// 「这个声明应解析到哪个注册键」（≠「它注册在哪个键下」——被擦除的 decl-only
// partial 没有后者，但有前者：另一碎片里实现所用的键）。
// 这是全仓库唯一保留裸名回落的地方。
public static string MethodKeyOf(StrMap methods, MethodDecl md) {
    if (md.RegKey != "") { return md.RegKey; }
    string arityKey = md.Name + "$" + md.ParamCount.ToString();
    if (methods.ContainsKey(arityKey)) { return arityKey; }
    return md.Name;
}
```

逐字复刻今天三档的语义（注意档序：现有代码先算 arityKey、再被 RegKey 覆盖、
最后才 `ContainsKey` 回落 —— 等价于上面的写法）。

> **原 A/B 之争降级为此处的实现细节**：写侧补齐后，被擦除的 decl-only partial
> 仍走 `md.RegKey == ""` 分支。是否顺手在擦除分支写 `md.RegKey = regName`
> （即原 B 案，一行，`regName` 已现成、恒 `md.Name`）**不影响可观察行为** ——
> 两条路径给出同一个值。建议**写**（少一个空洞、少一条分支被执行），
> 但这不再是需要单独裁决的方向问题。

### 3.3 消费点 #7 的处理

`IrGenMemberEmitter.z42:18-20` 不查表、只拼 IR 名，且其结果对 decl-only partial 无人使用。
直接改为 `md.RegKey`（写侧补齐后恒非空），删掉 ①③。

## 4. 验证策略

**无格式 bump → 本地可完整验证**（不必走两代自举）：

| 门 | 判据 |
|---|---|
| **字节对账** | 改动前后 `artifacts/build/compiler/*/dist/*.zpkg` 逐字节相同（BLID 除外）—— 「字节中性」的直接证明。**不通过就立即停下重新分析** |
| `xtask test compiler` | 自举不动点 gen1==gen2 3/3 |
| `xtask test` 全量 GREEN | 含 impl / cross-zpkg `impl_propagation` / `impl_reflect` / partial-types goldens |
| `xtask test stdlib --mode jit` 两 shard | **必跑** —— 本地默认只跑 interp；派发相关改动的失败模式是**静默派发到错误目标**而非崩溃（见 `local-green-misses-jit-and-lines`）|
| `xtask test bootstrap` | 上一版 nightly z42c 仍能编当前源 |

## 5. 开放问题

1. 收敛与移除回落是否分两个 PR（**建议分开**）。
2. `CallEmitter` 静态查找是否纳入后续 change（本变更明确不含）。
