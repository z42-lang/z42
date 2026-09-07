# Tasks: fix-imported-generic-func-fidelity

> 状态：🟢 已完成 | 创建：2026-09-08 | 完成：2026-09-08 | 类型：fix

**变更说明：** 修复**跨包调用泛型自由函数编不过**——`T IdOf<T>(T a)` 声明在包 A，包 B 里
`IdOf(7)` 报 `E0402: cannot assign int to T (argument)`，**显式类型实参 `IdOf<string>("gen")` 也不救**。

**原因（三段链路，两端都漏）：**
`IrFunction` **早就携带** `TypeParams` / `TypeParamCount`（承载位是 zbc SIGS 的 tp 块，
`add-reflective-invoke` 起写真实值，**无格式 bump**）。但：

1. `ExportedFuncZ` **根本没有型参槽**——`ExportedMethodZ` / `ExportedInterfaceZ` / `ExportedDelegateZ` 都有，
   唯独自由函数没有；
2. 于是 `TsigReconcile.Rebuild` 建 `ExportedFuncZ` 时**无处可搬**（兄弟函数 `_methodFromSig` 对方法
   是搬了的）；
3. 于是 `ImportedSymbolLoader` 的自由函数分支只能调**两参 `_resolve`**（无型参上下文），签名里的 `T`
   落到 `_resolve` 末尾 `Z42ClassType.Builtin(name)` 兜底 ⇒ **型参身份丢失**（成了名叫 `"T"` 的普通类）
   ⇒ `Conversion._classifyBuiltin` 分支 B「恰一侧含型参 → 擦除放行」永不触发 ⇒ E0402。

⭐ 这是 `add-argument-type-check`（#523）R1 **自己列出却漏修的第四处**——`_tpsWith` 的注释白纸黑字写着
「此前只喂类级或压根不喂（接口方法 / trait-impl 方法 / **自由函数**）」，那批修了前两处，自由函数漏网
（很可能正因为 `ExportedFuncZ` 当时没有槽可放）。

**根因修复（不打补丁）：** 把方法侧已经走通的那条链在自由函数上原样补齐，三处各一刀。

## 设计

- `ExportedFuncZ` 加 `TypeParams` / `TypeParamCount`（命名对齐 `ExportedMethodZ`）。
  🔴 **严守自举铁律**：ctor 元数**不变**（旧种子 ABI 冻结的调用约定不能被新增必填形参打破）、
  ctor 给默认值（`new string[0]` / `0`）、`TsigReconcile` **构造后赋值**。
  （先例：`ParamsFrom` / `IsSealed` / `IsDeprecated` / `ExportedMethodZ.TypeParamCount` 全是这个手法。）
- `TsigReconcile.Rebuild` 自由函数分支：`ef.TypeParamCount = f.TypeParamCount; ef.TypeParams = f.TypeParams;`
  （逐字镜像 `_methodFromSig:569-570`）。
- `ImportedSymbolLoader` 自由函数分支：`_tpsWith(new string[0], 0, fz.TypeParams)` 喂**四参** `_resolve`
  （自由函数无外层类 ⇒ outer 空，与 trait-impl 方法那支同形）；并把 `TypeParamCount` 填到
  `MethodSymbol` 上（供泛型 arity 重载过滤，镜像方法侧）。

**刻意不改的地方**：`ExportedTypeExtractor` / `FuncImplExtractor` / `IrDump` 这三个**本地**产出端不填新字段。
已核实：到达 `ImportedSymbolLoader` 的 `ExportedModuleZ` **只来自 `TsigReconcile.Rebuild`**
（`DepScan.z42:149/222` + `DepReconcile.z42:27`），本地产出端不在这条路上；动它们只会平添 golden 字节漂移风险。

## 文档影响

- `docs/book/src/compiler/source-compile.md`「残留洞」表：删掉「泛型**自由函数** | `ExportedFuncZ` 不带型参名」
  那一行（本 change 正是把它补上）。
- `docs/roadmap.md` Deferred 表：`imported-generic-func-type-param-fidelity` 标 ✅ 已解决。
- `src/tests/cross-zpkg/self_type_cross_pkg/main/src/Main.z42` 的注释（记着「跨包泛型自由函数今天调不了」）
  已过期，须更正。

## Scope（本 change 允许改动）

- `src/libraries/z42.ir/src/ExportedTypes.z42` — MODIFY：`ExportedFuncZ` +TypeParams +TypeParamCount
- `src/libraries/z42.ir/src/TsigReconcile.z42` — MODIFY：`Rebuild` 自由函数分支搬运型参
- `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` — MODIFY：自由函数分支喂型参上下文
- `src/tests/cross-zpkg/free_func_cross_pkg/` — MODIFY：加泛型自由函数回归用例
- `src/tests/cross-zpkg/self_type_cross_pkg/main/src/Main.z42` — MODIFY：更正过期注释
- `docs/book/src/compiler/source-compile.md` / `docs/roadmap.md` — MODIFY：文档同步
- `docs/spec/changes/fix-imported-generic-func-fidelity/` — NEW：本容器

## 任务

- [x] 1.1 复现确认（实测 `E0402: cannot assign int to T (argument)` ×2，显式类型实参同样炸）
- [x] 1.2 根因定位（`ExportedFuncZ` 无槽 → TsigReconcile 无处搬 → ImportedSymbolLoader 只能两参 `_resolve`）
- [x] 1.3 `ExportedFuncZ` 加槽（ctor 元数不变）
- [x] 1.4 `TsigReconcile.Rebuild` 搬运
- [x] 1.5 `ImportedSymbolLoader` 自由函数分支接型参上下文 + 填 `MethodSymbol.TypeParamCount`
- [x] 1.6 回归 fixture（`free_func_cross_pkg` 加 `T IdOf<T>(T a)` + 隐式/显式两种调用）
- [x] 1.7 退回对照（证明 fixture 是**真门**，非空测试）—— 见下
- [x] 1.8 文档同步（source-compile.md 残留洞表 / roadmap Deferred / 过期注释）
- [x] 1.9 完整 GREEN（含 `test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit` + `test bootstrap`）
- [x] 1.10 归档 + PR

## 退回对照（**同源**，2026-09-08）

新负例最容易变成空测试，故做了两道，第二道才是决定性的：

1. **种子对照**：nightly 种子 z42c（`bf2f8d26` = main−1）编同一 fixture → `E0402` ×2。
   ⚠️ 两侧差一个 commit（#530），有混淆余地，故不作为结论。
2. **同源对照（决定性）**：在**本 worktree 的同一份源码**上只回退**消费端一处**
   （`ImportedSymbolLoader.z42`；`z42.ir` 的两处修复**保留**）→ 单独 `build compiler` → 同一 fixture
   立刻复现**逐字相同**的两条诊断：
   ```
   main/src/Main.z42(16,28): E0402: cannot assign int to T (argument)
   main/src/Main.z42(17,36): E0402: cannot assign string to T (argument)
   ```
   ⇒ 新用例确实钉在被改的那条链上，且**隐式推断**（`IdOf(7)`）与**显式类型实参**（`IdOf<string>("gen")`）
   两种形态都真的在门内。

另有一条**逻辑反证**：若 `fz.TypeParams` 到达时为空，`_tpsWith` 返回空表 → `_resolve` 仍落
`Z42ClassType.Builtin("T")` 兜底 → 与修前完全等价、必然照旧 E0402。fixture 通过 ⇒ 型参名确实过了线，
排除「碰巧因别的原因绿」。

## GREEN 状态（本地，2026-09-08）

| 门 | 结果 |
|---|---|
| `xtask test`（完整，interp） | ✅ 全绿，含 z42c 自举**字节不动点** |
| `xtask test stdlib --mode jit` | ✅ 全绿 |
| `xtask test e2e --dir cross-zpkg --mode jit` | ✅ 全绿 |
| `xtask test bootstrap` | ✅ `NO staged-bootstrap boundary violation` |

`test bootstrap` 这道对本 change **是必须的**（不是走形式）：改动落在 `z42.ir` —— z42c **运行期自依赖**
的 stdlib 库（bootstrap-seed 轴 ③/④）。绿即证明「上一个 nightly 的 z42c 仍能编当前源」，
`ExportedFuncZ` 加字段没有踩 ctor 元数 ABI。

## 诚实记账：本 change **没有**做到什么

跨包泛型自由函数现在**与跨包泛型方法完全同档**——但那个档次本身仍有一个已知上限：

- 形参类型是**裸型参**时，实参依旧不检查。`IdOf<string>(7)` 这类「显式类型实参与实参不符」今天
  **仍无诊断**，因为 `Conversion._classifyBuiltin` 分支 B「恰一侧含型参 → GenericErase」放行。
  这是 Deferred **`tighten-bare-type-param-target-erasure`**（动的是**通用**擦除规则，爆炸半径未量），
  与本 change 正交、**继续开着**。
- 换言之本 change 修的是「**编不过**」（型参身份丢失 ⇒ 连擦除放行都不触发），不是「**检查更严**」。
  两者别混为一谈。
