# Design: `MethodDecl.RegKey` 单一真相源

> 配套 [proposal.md](proposal.md)。行号基于 `origin/main @ 44e67ef1`。

## 1. 事实基线（全部经代码核实）

### 1.1 字段与默认值

| 项 | 位置 | 值 |
|---|---|---|
| `MethodDecl.RegKey` 声明 | `src/libraries/z42c.syntax/src/Decl.z42:140` | `public string RegKey;`，注释即写「`""` = 未填（回落旧逻辑）」 |
| 默认值 | `Decl.z42:152` | 构造函数**显式** `this.RegKey = "";` |
| `MethodSymbol.RegKey` 默认 | `Symbol.z42:32` | **`= name`**（裸名，非空串）|

> 注意这处**非对称**：符号侧默认裸名、AST 侧默认空串。所以「RegKey 为空」只可能出现在 AST 侧。

### 1.2 赋值点穷举

| 位置 | 目标 | 覆盖范围 |
|---|---|---|
| `MemberCollector.z42:220` | `md.RegKey` | **唯一**给 AST 写的地方；仅 `_fillClass`（class/struct）的 `c.Members`，且被 `:206` 的 decl-only partial 擦除分支排除 |
| `InheritanceResolver.z42:120-121` | `localSym.RegKey` + `md.RegKey` | override 槽对齐**改写**；前置 `ct.Methods.Get(md.RegKey)` 非空（`:115`）→ RegKey 为 `""` 时 `Get("")` 必 null → **不会把空串变非空** |
| `ImportedSymbolLoader.z42:303` | `sym.RegKey` | imported 符号，**只写符号不写 AST** |

### 1.3 留空的两类（可达消费点）

**(A) impl-block 方法 —— 恒空**

证明链：
1. `MemberCollector._passMembers`（`:17-28`）只分派 `ClassDecl` 与顶层 `MethodDecl`，**不处理 `ImplDecl`**。
2. impl 方法符号由 `InheritanceResolver._passImpls`（`:33`）注册：
   `target.Methods.Put(md.Name, this._sc._methodSymbol(...))` —— 用**裸 `md.Name`**，**不写 `md.RegKey`**。
3. 无第三处赋值（§1.2）。

现场用例：`src/tests/generics/extern_impl_user_class.z42`、`src/tests/cross-zpkg/impl_propagation/`、
`src/tests/cross-zpkg/impl_reflect/`。

**(B) decl-only partial method —— 恒空**（`MemberCollector.z42:205-208` 擦除分支）
现场用例：`src/tests/partial-types/partial_method.z42`。

### 1.4 留空但**不可达**这些消费点的三类（不必处理）

| 类别 | 为何不可达 |
|---|---|
| 接口方法 | `_fillInterface`（`:39-41`）裸名注册；`_bindClass` 只对 class/struct 调（`TypeChecker.z42:312`）；导出走独立的 `ClassExtractor._extractInterface`（`:12-31`，恒裸名） |
| 顶层自由函数 | 走 `DeclBinder._bindFreeFunc`（`:354`）与 `_extractFunc`，不经这些点 |
| 合成 `MethodDecl` | `IrGenMemberEmitter:69/82/117/135`、`StubEmitter:166`、`AttributeSynth:139`、`BenchmarkDesugar:82/112` 等——全部**直接**交 `FunctionEmitter.EmitFunction` 并显式给定 IR 名，从不进 `EmitMethod` |

## 2. 消费点逐一判定

| # | 位置 | 现状依赖 | 补 RegKey 后 |
|---|---|---|---|
| 1 | `ClassExtractor.z42:194-196`（祖先/自身实例方法导出）| **依赖 ③ 裸名回落**：跨-CU partial 时 `classMap` 是 per-CU last-wins（`ExportedTypeExtractor.z42:111-120`），decl 碎片所在 CU 靠回落才能把方法写进该 module 的 TSIG | 可删 ①③，**但需先补跨-CU partial 测试**（现零覆盖）|
| 2 | `ClassExtractor.z42:328-330`（自有静态方法导出）| 无 `HasBody` 卫兵 → `static partial` decl-only 可达；但**全仓库无此写法** | 可删，需补 `static partial` 用例 + TSIG 对账 |
| 3 | `DeclBinder.z42:188-190`（`_bindImpl`）| **完全依赖 ③**：impl 方法 RegKey 恒空 → `mkey = "Hello$0"` miss → 回落 `"Hello"` 命中 `_passImpls` 的裸键 | 补 (A) 后 ② 直接命中 → 可删 ①③ |
| 4 | `DeclBinder.z42:237-239`（`_bindClass` 方法体绑定）| **已是死代码**：有 `if (md.HasBody)` 卫兵（`:234`）排除 decl-only partial；且下一行 `md.IsCtor && !ms.IsStatic`（`:245`）**无条件解引用 `ms`** —— 若真有漏网早该 NPE，自举常绿即反证 | 可直接删（无需前置）|
| 5 | `DeclBinder.z42:336-338`（`_checkExposure`）| 无 `HasBody` 卫兵 → decl-only partial 可达；删后该碎片跳过 E0441 检查 | 诊断不丢失（impl 碎片会查），但**重复条数会变** → 需对账 golden |
| 6 | `IrGenTypeEmitter.z42:122-124`（`EmitImpl`）| 与 #3 **成对**：#3 产 body 键、#6 消费同键拼 IR 名 `g._qClass(itn) + "." + imk`（`:129`）| 必须与 #3 同去同留 |
| 7 | `IrGenMemberEmitter.z42:18-20`（`EmitMethod`）| 键在 `:18-20` 算、`HasBody` 到 `:22` 才判 → decl-only partial 能走到键计算，但后续三分支（`HasBody`/`extern`/`abstract && !static`）对它**全不成立** → 算出的 `irName` 无人用 | 可直接删 |

> **单独处理**：`CallEmitter.z42:238-243` 的静态 DepIndex 查找。其 `arityKey = MethodName + "$" + ArgCount`
> 与 `DependencyIndex.AddModule`（`z42.ir/src/DependencyIndex.z42:88-123`）注册的四种键
> （`Cls.<全串>` / `Cls.<bare>` / `ns.Cls.<全串>` / `ns.Cls.<bare>`）**都不匹配**，当前是死路
> （真正命中的是 `:241/:243` 的全键探测）。但跨版本自举时旧 nightly 产物可能仍是 `Name$arity` 形态，
> 且该段注释反复强调「字节不动」→ **不在本变更范围**。
>
> 另需修正一个常见误解：静态方法**并非**恒全 mangle——`MemberCollector.z42:177` 的
> `staticVirtual = mst && (override || abstract)` 走基线裸键（INumber 的 `op_*` 等），
> 理由见 `:170-173`（VCall 按运行时类型派发，base 与派生必须同键）。

## 3. 方案

### 3.1 impl 方法（必做，字节中性）

`InheritanceResolver._passImpls`（`:33` 附近）注册时补写：

```z42
MethodSymbol isym = this._sc._methodSymbol(table, md, targetName, new string[0], 0);
md.RegKey = md.Name;          // ← 新增：与下一行 Put 的键一致
isym.RegKey = md.Name;        // ← 符号侧本就默认 = name，显式化以免将来默认值变动
target.Methods.Put(md.Name, isym);
```

**为什么字节中性**：消费点 #3 / #6 的兜底 ③ 算出的正是 `md.Name`；改后 ② 命中同一个值。

> **设计权衡**：impl 方法目前**不参与 primary/非-primary 规则**（恒裸名，不支持同名重载）。
> 本变更**保持这一现状**——把它并入新规则是独立的语义扩展（会 rekey、需格式 bump），不在此列。
> 这一点必须在代码注释里写明，否则后人会以为 RegKey 已统一而误改。

### 3.2 decl-only partial（二选一，待 User 裁决）

- **A 案（保守，推荐）**：**不写** RegKey，保留消费点 #1/#5 的兜底。
  理由：decl-only partial 被**擦除**（不注册符号），写一个指向不存在符号的键没有意义；
  #1 的裸名回落是靠它去命中**另一个碎片**注册的符号，语义上不是「本方法的键」。
  代价：不变量变成「**除被擦除的 decl-only partial 外**，RegKey 恒非空」——需在 `Decl.z42:141` 注释写明。
- **B 案（彻底）**：擦除分支也写 `md.RegKey = <该方法若不被擦除时会得到的键>`。
  代价：需在擦除路径里复算 primary/非-primary，逻辑重复且易与 `emittedInst` tracker 不一致。

**建议 A 案** —— 它让不变量有一个**明确且可陈述**的例外，而不是引入一个语义可疑的键。

### 3.3 兜底移除（第二步，独立 PR）

按 §2 的判定分批：
- **批 1（无前置）**：消费点 #4、#7 —— 已证死代码。
- **批 2（需前置测试）**：#3 + #6（成对，前置 = §3.1 落地并验绿）。
- **批 3（需补测试）**：#1（跨-CU partial TSIG）、#2（`static partial`）、#5（E0441 条数对账）。

## 4. 验证策略

**本变更无格式 bump → 本地可完整验证**（不必走两代自举）：

| 门 | 判据 |
|---|---|
| `xtask test compiler` | 自举不动点 **gen1==gen2 逐字节** —— 这是「字节中性」的直接证明 |
| `xtask test` 全量 GREEN | 含 e2e goldens（impl / cross-zpkg impl_propagation / impl_reflect / partial-types）|
| `xtask test stdlib --mode jit` 两 shard | **必跑** —— 本地默认只跑 interp，派发相关改动必须覆盖 JIT（见 `local-green-misses-jit-and-lines` 教训）|
| `xtask test bootstrap` | 上一版 nightly z42c 仍能编当前源 |

**关键对账**：改动前后，`artifacts/build/compiler/*/dist/*.zpkg` 应逐字节相同（BLID 除外）。
若不同 → 「字节中性」假设被推翻，立即停下重新分析。

## 5. 开放问题

1. §3.2 的 A/B 取舍。
2. 兜底移除是否与补 RegKey 同 PR。（建议分开：补齐纯加性、可快速合并；移除需前置测试）
3. `CallEmitter` 静态查找是否要一并纳入后续 change（本变更明确不含）。
