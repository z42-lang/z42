# Design: 接口满足性真洞修复

> proposal 见同目录。记三部分的实现机理与决策权衡。

## P0 机理与修法

`ForwardGenerator` 的 4 处 `"E0463"` 是「`[Forward]` 特性形态错误」，与 `ConstraintMethodArgTypeParam`
（#604 的 E0463）无关。常量表 SoT 已把 E0463 给 #604。修法：
- DiagnosticCodes 加 `ForwardMalformed = "E0468"` + 注释（四子类：字段类型无成员面 / 实参非
  typeof·methodof / typeof 非接口 / methodof 成员不在字段类型上）。语义层用字面量发码（同族纪律）。
- ForwardGenerator 4 处 `"E0463"`→`"E0468"`。若 ForwardGenerator 的测试按码断言，同步改。
- grep 全仓确认无「E0463 用于 Forward」残留。

## #1 机理与修法

### 为什么 MangleKey 不该直接加 IsStatic/Ret

`MangleKey` 是**重载派发键**（[OverloadResolver.z42:66]），全仓用于定义点+跨包 import round-trip、
且编进 RegKey / 运行期派发。**往里加 IsStatic/Ret 会改派发键 → 撼动自举字节**（同 #604「Object 优先级
不能反」的教训）。⇒ 种类/返回校验**不进 MangleKey**，而在 `_checkOneIfaceMethod` 里 MangleKey 命中**之后**
单独比。

### 两段追加校验

`_checkOneIfaceMethod` 现逻辑：构造 `wantKey`（Self 替换后形参）→ 沿实现类 base 链找 MangleKey 相等的
`cm` → 命中即 `return`（判满足）。改为：命中 MangleKey 后**不立即 return**，先比：

1. **种类**：`cm.IsStatic != ims.IsStatic` → E0412「种类不符」。
2. **返回类型**：`wantRet = _substForIface(ims.Signature.Ret, ct, it)`（复用已有替换，Self→实现类）。
   兼容判据（retOk）= 以下任一：
   - Unknown/Error 任一侧 → 吸收放行（imported 未解析返回不假红）。
   - **同类型** `OverloadResolver.TypeKey(implRet) == TypeKey(wantRet)` ——**与 MangleKey 比形参同一口径**。
   - **协变** impl 是 wantRet 的**子类 / 接口实现**：`table.IsSubclassOf(implName, wantRet.Name())
     || table.Implements(...)`（仅 class/instantiated）。

   🔴 **为什么不用 `IsAssignableTo`（踩过两次）**：① 它**不带符号表**、认不了子类关系（`Dog`→`Animal`
   返 false，协变守卫当场红）；② 它对 `String[]` vs `string[]` **反而认不出**（wrapper 名 vs keyword
   名的数组元素不等），而这俩**是同一类型**（`Z42cReplCompiler` 实现 `IReplCompiler` 正是这形态）→ 假红。
   `TypeKey` 把 `String→string`/`Int32→int`/数组叶子 keyword 化，和形参比较完全同口径，才是「同类型」的
   正确判据；协变另用 `table` 补。代价：`IsAssignableTo` 会放行的**数值拓宽返回**（`int M()` 实现
   `long M()`）现按不同类型处理——这是对的（返回类型真不同），且实测 stdlib 零此形态。

### 🔴 只对本包接口校验（`!it.IsImported`）—— 新照亮一个 latent bug

kind/返回校验**只在本包接口**（`!it.IsImported`）上跑。跨包接口成员的 `IsStatic` 在导入侧**不可靠**：
`static abstract` 成员经导出元数据/wire 过来 `IsStatic` 丢成 false（`ImportedSymbolLoader:388` 拿到的
`mz.IsStatic` 本身即 false）。不跳则 `struct Money : INumber`（INumber 导入自 z42.core）的 5 个
`static override` 全被误判「接口声明为 instance」→ 假红（`src/tests/operators/static_abstract_operator.z42`
当场炸 5 条 E0412）。

⭐ **这是本 change 新照亮的既存缺陷**（[[silent-feature-masks-other-bugs]] 模式第 N 次）：**导入接口的
静态抽象成员丢失 IsStatic**。端到端修需动导出侧 + 可能格式 bump，超出本 change「无格式 bump」范围 ⇒
登记 Deferred `imported-iface-static-member-fidelity`。保守跳过 = 跨包接口 kind/返回**漏报**（与
func-type/assoc-type 跨包漏报同取舍），本包接口（真静默洞所在）仍全查。

⚠️ **测量盲区的教训**：阶段 0 只量了 `build stdlib`+`build compiler`，**漏了 `src/tests/*.z42` 测试语料**
——而 `static_abstract_operator.z42` 的 `struct Money : INumber` 正是**导入接口**场景（stdlib 里 Int32:INumber
是**本包** INumber，测不出导入侧缺口）。⇒ **量接口满足性类改动的爆炸半径，必须把 `build test`（golden 语料
编译）也纳入**，不能只看 stdlib/compiler 自建。

### 🔴 血泪教训：`build` 的增量缓存会给出假阴性的爆炸半径

阶段 0 第一次测量（IsAssignableTo 版）报「stdlib+compiler E0412 = 0」**是假的**——`build compiler` 的
增量缓存**没重编 z42c.pipeline**，`Z42cReplCompiler` 的 `String[]` vs `string[]` 从没过检查。换判据后
重编才炸出来。⇒ **量新诊断爆炸半径必须 `rm -rf artifacts/xtask/.cache` 后全量重编**，别信带缓存的
「0 命中」（呼应 [[local-green-misses-jit-and-lines]] 族的「缓存掩盖」）。真·干净测量：build+stdlib 各 0。

两项都过才 `return`（满足）；任一不过发 E0412 并 `return`（已报，不再走到末尾的「no overload matches」）。

### 为什么仍用 E0412 不新造码

种类/返回不符与「缺成员 / 形参不匹」同属「接口没被正确实现」语义族，E0412 `InterfaceMismatch` 正是这个
伞。给**具体消息**区分即可，不新造码（避免码爆炸；与 #604 给独立语义新造码的判断不矛盾——那是不同语义族）。

### 爆炸半径风险（阶段 0 必量）

新校验在接口满足性 pass 跑、覆盖 stdlib + compiler 全部 `class C : I`。风险点：
- 协变返回的现存实现（若有）被精确规则误伤 → 故采协变允许。
- 真·种类/返回不符的现存实现（既存 bug 被照亮）→ 逐条评估修源。
- imported 接口方法的 `ims`（无 Decl）——`IsStatic` 有（ImportedSymbolLoader 还原）、`Ret` 有；但若
  某些 imported 返回类型退化成 Unknown，`IsAssignableTo` 要放行。阶段 0 实测定。

## #2 机理与修法

`_satisfiesBase`/`_satisfiesParamRef` 开头加：
```
if ((arg is Z42GenericParamType) || (arg is Z42ErrorType) || (arg is Z42UnknownType)) { return true; }
```
与 `_satisfiesInterface`（:487-490）逐字同构。理由：型参实参在**外层实例化点**已被校验（泛型类内部
转发 `new Inner<T>()` 时 `T` 尚不透明，此处放行、留给 `Outer` 被实例化时的检查）。

纯放松 ⇒ 对现有全绿代码零新诊断。价值靠新 fixture 兜（`Outer<T>` 转发 base-约束 `Inner<T>`）。

## 零漂移论证

P0 纯码重编号；#1 只在满足性 pass **新增诊断**（不改绑定/发射），build 源若零新错则 zbc 逐字节不变；
#2 纯放松（移除假红）不改发射。三者均无格式 bump。自举不动点 3/3 兜底。
