# Tasks: fix-inferred-type-arg-not-resolved

> 状态：🟢 已完成 | 创建：2026-09-09 | 完成：2026-09-09 | 类型：fix

**变更说明：** 修复**基元类型实参在「省略尖括号」的泛型调用上不满足接口约束**——

```z42
T Double<T>(T x) where T: INumber { return x.op_Add(x); }

Double<int>(21);   // ✅ 过
Double(21);        // ❌ E0402: type argument `int` for `T` does not satisfy constraint `INumber`
```

一个尖括号的差别。`int` 明明 `struct Int32 : IComparable, IEquatable, INumber`。

## 根因

内建基元在 z42c 里**两种拼写并存**（`unify-value-types` 阶段 3 删掉 `Z42PrimType` 后的遗留）：

| 来源 | 类型对象 | `Name()` |
|---|---|---|
| 表达式类型（`21` 的 `.Type()`） | `Z42ClassType.Builtin` 轻量合成 | **关键字** `"int"` |
| 显式 `<int>`（`env.ResolveType` → `SymbolTable.BuiltinType`） | `Classes` 表里的**包装类** `Std.Int32` | `"Int32"` |

`ConstraintChecker._satisfiesInterface` 判定走 `symbols.Implements(arg.Name(), iface)`，
而 `Implements` 第一步是 `this.Classes.Find(cur)` —— 表里的键是 **`Int32`**，
喂 `"int"` **恒 miss** → break → 返回 false → 报「不满足」。

`add-generic-type-arg-inference`（#536）阶段 C 把**推断结果**接上了 `ConstraintChecker.CheckMethod`
（关掉 Deferred `where-constraint-future-inferred-method-args`），而 `TypeArgInference.Infer` 的绑定值
直接取自 `args[i].Type()` —— 即上表第一行那种形态。显式路径经 `env.ResolveType` 归一过，推断路径没有。
⇒ **接上校验的那一刻，「推断 + 基元」这一格就全错了**，且此前推断路径根本不做约束校验，所以是净新增的假红。

**四格实测**（确认缺口就是归一，不是约束判定本身坏了）：

| | 显式 `<T>` | 推断 |
|---|---|---|
| 用户 struct `MyNum : INumber` | ✅ | ✅ |
| 基元 `int` | ✅（`<int>` / `<Int32>` 都过） | ❌ |

## 为什么没人发现

这三个受害文件全在 golden 语料里（`src/tests/generics/generic_inumber.z42`、
`generic_primitive_interface.z42`、`src/tests/operators/static_abstract_operator.z42`，共 18 条），
而 golden 编译走 `z42c --emit-zbc` —— 那条路**丢弃全部诊断、exit 0 照写产物**
（`restore-emit-zbc-diagnostics` 正在修的洞）。测试运行期正常通过，编译期的 18 条错误没有任何人看见。

⚠️ 但它**不只影响 golden**：同一份源码放进最小工程走 `z42c build` 报一模一样的错（已实测）。
也就是说**今天任何用户工程写 `where T : INumber` + 省略尖括号就是编不过的**。

## 根因修复

在**推断出口**归一，而不是在判定端逐个打补丁：

`TypeArgInference.Infer` 增 `SymbolTable symbols` 首参；边界 ① 校验通过后逐位过
`_resolvedForm(symbols, bound[k])` —— 类型是内建（`IsBuiltinType()`，码 0..13）就换成
`symbols.BuiltinType(name)` 拿到的那个包装类，与显式路径**同形**。

- 归一放出口而非放 `_satisfiesInterface`：后者只是众多消费方之一（还有 `_satisfiesBase` /
  `_satisfiesParamRef` / 阶段 B 的形参位代换 `_checkSubstMethodArgs`），逐个打补丁等于承认
  「型参实参有两种形态」；出口归一才是让两条路彻底同形的那一刀。
- 门取 `IsBuiltinType()`（0..13）而非 `IsScalarType()`（0..11）：`string` 是 12、`object` 是 13，
  而 `where T : IComparable` 下的 `Max("a", "b")` 正落在 string 上。
- `symbols == null` 时原样返回（`Infer` 的纯函数性质保留，单测可脱 SymbolTable 调用）。

**刻意不动的**：`bc.MethodTypeArgs` 仍然**不回灌**（#536 design D4 的裁决不变）——本修复只让
推断结果在**诊断口径**上与显式一致，不改发射。⇒ 零字节漂移（自举不动点不受影响）。

## 文档同步

- `docs/book/src/language/generics.md` §类型实参推断：新增「**不变式：推断出的类型实参必须与
  显式写出的同形**」小节（两种拼写并存的对照表 + 为什么归一放出口）。
- 同页 §限制 更正一条**失效陈述**：「primitive 类型未实现 interface，`Max<int>(1,2)` 暂不可用」——
  `Primitives/` 下每个 wrapper 都写着 `struct Int32 : IComparable, IEquatable, INumber`，
  该限制在实现落地后一直没撤。（`Max(1,2)` 当时确实报错，但那是本 change 修的归一缺口，不是该限制。）
- 页头「对齐」日期刷新到 2026-09-09。

## 验证

- [x] 最小复现四格实测（用户 struct / 基元 × 显式 / 推断）——只有「推断 + 基元」这一格坏
- [x] 三个受害 golden（`generic_inumber` / `generic_primitive_interface` /
      `static_abstract_operator`）经 `z42c build` 路径由 18 条 E0402 → **0**
- [x] 单测：`generic_inference_tests.z42` 新增 4 条（源码自带 `struct Int32 : I` / `struct String : I`，
      在无导入符号的 harness 里复现归一）。**做过退回对照**（撤掉 `_resolvedForm` 那一行 →
      `build compiler` → `test compiler`）：
      | 用例 | 撤改动后 | 作用 |
      |---|---|---|
      | `..._inferred_prim_..._satisfies_...` | **FAIL** | 真门（差分） |
      | `..._inferred_string_..._satisfies_...` | **FAIL** | 守 `IsBuiltinType` vs `IsScalarType` |
      | `..._explicit_prim_..._satisfies_...` | PASS | 对照组（前后都绿）|
      | `..._still_reports_unimplemented_interface` | PASS | 守「别修成从不响的门」|
- [x] `xtask test` 全绿（12 stages，3m08s，`✅ GREEN — all stages passed`）
- [x] 自举不动点 3/3（`gen1==gen2`，本改动不入发射路径 ⇒ 零字节漂移，符合预期）

> ⚠️ 踩坑记一笔：新 worktree 从旧 sha 供种后，`artifacts/xtask/xtask.zpkg` **不按 mtime 自动重建**，
> 旧 runner 的 `_gateStageNames()` 只有 10 个 stage、而源码文档已有 12 → 防漂移门当场假红。
> **跨 sha 供种后必须手动 `z42c build scripts/xtask.z42.toml --release` 重建 xtask。**
