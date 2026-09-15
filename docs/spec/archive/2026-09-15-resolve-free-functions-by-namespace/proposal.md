# Proposal: 本包自由函数按命名空间解析

> **状态：🟢 已实施（User 2026-09-15 确认 DRAFT）** | 创建：2026-09-15
> 与 `add-call-arity-diagnostics`（PR-1）同一轮 DRAFT，拆为独立 PR。来源：#652 登记的编译器缺口 ②
> （xtask `_driverZpkg` 同名函数跨文件重复）。

## Why

### 本包自由函数表按裸名、后写覆盖

`SymbolTable.Functions` 以**裸名**为键，`MemberCollector` 注册时 `Put` 后写覆盖；而发射端 `QualifyFreeFunc`
一律按**调用方当前 ns** 限定。两边口径不一，实测三种坏形态（当前 main 自建编译器）：

| 形态 | 代码 | 结果 |
|------|------|------|
| B-1 同 ns 跨文件同名 | 两个文件都在 `ns X` 声明 `f` | 类型检查只见后写那份，两份发成同一 FQN，运行期只剩一份——**静默** |
| B-2 跨 ns 同名 | `Alpha` 有 `int f(int)`，`Beta` 有 `string f()`；Alpha 里 `int r = f(41)` | 文件名 `a.z42`/`b.z42` → **假报 E0402**（绑到了 Beta 的签名）；把 a 改名 c → 编译通过。**合法代码成败取决于文件名** |
| B-3 跨 ns 调用 | `Beta` 有 `g`，Alpha 里 `using Beta; g(1)` | 编译通过，运行期 `MissingSymbolException: undefined function Alpha.g`（**写了 using 也不行**） |

跨**包**的自由函数反而是好的（`ImportedFuncNs` 按源 ns 限定 + E0601/E0606/E0456 判歧义）——同包缺的正是这一套。


## 普查（全仓 GREEN 状态下挂探针：本包同名自由函数重复注册）

| 探针 | 命中 | 结论 |
|------|------|------|
| B 同名重复注册 | 19 | 同文件（已有 E0408 覆盖；含 known-broken `patterns.z42` 的解析残渣、REPL 单测）；**跨 ns 合法同名**：multi-exe 用例的多个 `Main`（`two_mains`/`ns_same_short_name`/`CacheMulti`）、z42c.semantics 四个单测文件各自的 `bodyDiags`/`countCode`/`pre`（**签名碰巧相同才没炸**）；**同 ns 跨文件：0** |

⇒ **只做「跨文件重名报错」会误伤 multi-exe 与单测**——跨 ns 同名是合法的，必须按 ns 解析。

## What Changes

（`compiler`，属名字解析规则修正）

- 符号表并存 **FQN 视图**（`FunctionsByFqn`，同 fix-type-ref-ns-collision（#353）`ClassesByFqn` 手法），本包注册写 `ns.name`
- 调用点裸名解析：在**调用方可见 ns 集**（本 ns + usings + 全局 ns）内查，与跨包自由函数 / 类型裸名**同一口径**；
  多个可见 ns 各有一份 → 复用 E0456 歧义判据；都没有 → E0401
- 发射端用**已解析符号的 ns** 限定（不再一律当前 ns）⇒ B-3 修好
- **同 ns 跨文件同名**（同 FQN 两份）→ E0408，诊断同时给出两处位置；同文件那条既有检查保持
- 字节影响：只改变「此前编不过 / 运行期炸」的形态；自举不动点、stdlib 产物预期不变（以 GREEN 与字节对账为准）
- fixture：B-1/B-2/B-3 各一条 + 文件名换序对照（B-2 必须两种文件序结果一致）；`book` 补命名空间页的自由函数解析规则


### 实施中确定的细节

- 可见集为空时（没写 `using`）退到**全部声明**：唯一即它（与类型裸名一致，不因少写 `using` 而报错）；
  多份同样报 E0456（此前该形态按裸名撞赢家、不报）
- 删除 `EmitContext.QualifyFreeFunc` / `ImportedFuncNs` / `CuPreprocess._filterShadowedFuncs` /
  `ImportedSymbols.Functions`·`FunctionNamespaces`——发射端不再按名字猜 ns
- 顺带修正：方法组转换（`IntFn h = g`）此前一律按当前 ns 发 `LoadFn`，引用导入 / 别的 ns 的函数同样发出不存在的名字

## 格式 / 种子影响

- 无 zbc/zpkg 格式变更、无新语法 ⇒ 不需要两-nightly
- 对原本正确的代码解析结果与旧猜测逐字相同 ⇒ 预期 stdlib 产物字节不变（以字节对账为准）
