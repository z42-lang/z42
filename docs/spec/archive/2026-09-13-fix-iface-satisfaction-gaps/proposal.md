# Proposal: 接口满足性真洞修复（E0463 碰撞 + 种类/返回类型校验 + base 约束假红）

> change: `fix-iface-satisfaction-gaps` ｜ scope: `compiler` ｜ 预期无格式 bump
> 来源: 本轮「泛型约束 gap 扫描」三路 Explore 结果（均在 origin/main `e96522990` 上核实）
> 三个独立逻辑单元、三个 commit、一个 PR（User 裁决「一起」）

## 背景：这是 gap 扫描挖出的三条，各自是一个逻辑单元

| 部分 | 性质 | 简述 |
|---|---|---|
| **P0** | 纯 bug 修复 | **E0463 一码两义**——我的 #604 与 #610 并行开发各抢了 E0463 这个空号 |
| **#1** | 语义变更（新诊断触发） | 接口满足性 **不比 static/instance、不比返回类型** ⇒ 静默放行错误实现 |
| **#2** | 语义变更（放松检查） | `_satisfiesBase`/`_satisfiesParamRef` 对未解析型参**假红**（与兄弟检查口径不一致） |

---

## P0：E0463 一码两义（确认的真 bug）

当前 main 上 E0463 被两个无关诊断共用：
- 常量表 [DiagnosticCodes.z42:164](src/libraries/z42c.core/src/DiagnosticCodes.z42#L164) `E0463 = ConstraintMethodArgTypeParam`（#604，型参收者约束方法实参校验）。
- `ForwardGenerator` **4 处发字面量 `"E0463"`**（[:104](src/compiler/z42c.semantics/src/ForwardGenerator.z42#L104) 字段类型无成员面 / [:126](src/compiler/z42c.semantics/src/ForwardGenerator.z42#L126) 实参非 typeof·methodof / [:163](src/compiler/z42c.semantics/src/ForwardGenerator.z42#L163) typeof 非接口 / [:208](src/compiler/z42c.semantics/src/ForwardGenerator.z42#L208) methodof 成员不在字段类型上）——是「`[Forward]` 特性形态错误」这**第五类** Forward 错误，**常量表里没有对应条目**（Forward 的其它类别是 E0464/E0465/E0466/I0467）。

⇒ 按码筛诊断会混淆两类完全无关的错误。并行开发抢空号的经典碰撞（我 #604 检查 E0463 free 时 #610 尚未合）。

**修法**：常量表是 SoT、E0463 归 #604；Forward 这 4 处是**无常量的字面量** ⇒ 给它们一个自己的新码 **E0468 = `ForwardMalformed`** + 常量（下一个空号：E0464–E0466 是 Forward、I0467 是 Forward info，0468 空）。改 4 处字面量 `"E0463"`→`"E0468"`。纯诊断码重编号，零行为改动、无格式 bump。

---

## #1：接口满足性不比 static/instance、也不比返回类型（真静默洞）

`InheritanceResolver._checkOneIfaceMethod`（[:373](src/compiler/z42c.semantics/src/InheritanceResolver.z42#L373)）用 `OverloadResolver.MangleKey`（[OverloadResolver.z42:66](src/compiler/z42c.semantics/src/OverloadResolver.z42#L66)）比对实现方——**MangleKey 只含 `name + paramCount + 各形参 TypeKey`，不含 `IsStatic`、不含 `Ret`**。后果：

```z42
interface INum { static Self op_Add(Self a, Self b); }
class Foo : INum { public Foo op_Add(Foo a, Foo b) { ... } }   // instance，漏 static → 今天静默通过

interface IClone { int Id(); }
class Bar : IClone { public string Id() { ... } }              // 返回类型不符 → 今天静默通过
```

两者 `wantKey` 与实现方 MangleKey 逐字相等 ⇒ E0412 不触发。`MethodSymbol` **两侧都有 `IsStatic`**（本地 `SymbolCollector` 设、导入 `ImportedSymbolLoader` 还原）——⭐ **所以这不是「卡在缺 `IsAbstract` 槽」**（gap 扫描推翻了这个旧判断），而是**匹配逻辑单纯没比**。`Signature.Ret` 两侧也都在。

**修法**：`_checkOneIfaceMethod` 在 MangleKey 命中后**追加两项校验**：
1. **种类**：`cm.IsStatic == ims.IsStatic`，不等 → 报（种类不符）。
2. **返回类型**：`cm.Signature.Ret` 对 `_substForIface(ims.Signature.Ret)` 的**兼容性**——见下语义裁决。

复用 **E0412**（`InterfaceMismatch`，同一语义族「接口没被正确实现」），给**具体消息**区分种类/返回不符，不新造码。

### 需 User 裁决（#1）

- **返回类型规则：协变允许 vs 精确相等**（推荐**协变允许**）：接口声明 `Animal Make()`、实现 `Dog Make()`（Dog : Animal）对调用方**类型安全**（拿到 Dog 当 Animal 用成立）。⇒ 判据 = `impl.Ret.IsAssignableTo(want.Ret)`（含精确相等 + 协变 + Self 替换后的具体类型）。精确相等会**误伤合法协变实现**。采协变允许则假阳性面更小。
  - ⚠️ **必须阶段 0 实测**：stdlib/compiler 现有接口实现里有没有返回类型与接口声明不同、今天「碰巧过」的（协变或真错）。>0 则逐条分类，真错修源、协变则验证协变规则放行。

---

## #2：`_satisfiesBase`/`_satisfiesParamRef` 对未解析型参假红

[ConstraintChecker.z42:501](src/compiler/z42c.semantics/src/ConstraintChecker.z42#L501) `_satisfiesBase` 只认 `Z42ClassType`/`Z42InstantiatedType`，其余（含 **`Z42GenericParamType`**）一律 `return false` → `_err`。`_satisfiesParamRef`（[:508](src/compiler/z42c.semantics/src/ConstraintChecker.z42#L508)）同样对型参实参可能走 `IsAssignableTo` 假红。

这与 `_satisfiesInterface`（[:487](src/compiler/z42c.semantics/src/ConstraintChecker.z42#L487)）/`_isEnumArg`/`_hasNoArgCtor`/`_checkAssocBinding` 全都**放行未解析型参**的口径**不一致**。后果：`class Outer<T> where ...` 内部转发 `new Inner<T>()`、而 `Inner` 带 **base-class 约束**时，实参 `T` 是 `Z42GenericParamType` → base 约束判 false → **假红**（用户难修，报错指向看似合法的实例化）。

**修法**：`_satisfiesBase`/`_satisfiesParamRef` 开头加「`arg is Z42GenericParamType`（或 error/unknown）→ `return true`」放行，与兄弟检查统一口径。放行的型参由其**外层实例化点**校验（同 `_satisfiesInterface` 的既有理由）。

### 边界（#2）

- 这是**放松**（移除假红），只会**减少**诊断、不会新增 ⇒ 对现有 stdlib/compiler 零行为改动（它们今天就全绿，说明要么无此形态、要么未命中）。∴ 这是**潜在**假红的修复——价值在**新 fixture** 证明「`Outer<T>` 转发 base-约束 `Inner<T>`」现在能编。

---

## 验证要点（全 change）

- **阶段 0 爆炸半径**（#1 必做）：实现后 `build stdlib`+`build compiler` 全量，看新 E0412 触发几条、逐条分类（真错 / 协变误伤 / 种类真洞）。>0 真错则那是**既存 bug 被照亮**（本线反复的形状），单独评估修源。
- **编译期负例门**（`z42c.semantics` 单测，`SemanticDump`/`bodyDiags` harness —— golden/`--emit-zbc` 吞诊断）：
  - #1：static 实现成 instance → E0412；返回类型不符 → E0412；协变返回 → **不报**（无误伤守卫）；种类/返回都对 → 不报。
  - #2：`Outer<T>` 转发 base-约束 `Inner<T>` → **0 诊断**（修前假红对照）。
  - **退回对照**坐实每条修前 FAIL / 修前假红。
- **P0**：改码后 ForwardGenerator 的负例测试断言从 E0463 改 E0468（若有），grep 全仓无残留「E0463 用于 Forward」。
- **零漂移 / GREEN**：`xtask test` 全 stage + 自举不动点 3/3；`test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit`；`test bootstrap`（无格式 bump）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
