# Proposal: 泛型推断的数值最佳公共类型

> change: `generic-inference-best-common-type` ｜ scope: `compiler` ｜ 无格式 bump
> 来源: [[add-associated-types-program]] 第五批 (#536) 的 Deferred `generic-inference-best-common-type`

## Why（gap 已在今天的 main 上重新核实，非照抄 Deferred）

#536 加的方法级类型实参推断在 `TypeArgInference._unify`（`TypeArgInference.z42:113`）里按位合并型参绑定，
**首次赢**、遇到不同类型即返回 `false` → `Infer` 返回 `Ok=false` → `MemberResolver._applyMethodTypeArgs`
静默跳过（不校验 where、不查实参）。后果：`T Max<T>(T a, T b)` 调 `Max(1, 2L)`（int 与 long 冲突）
**推断失败、零诊断、零检查**——省略尖括号的混合数值泛型调用今天全部静默漏过。

- 这是 #536 有意留的边界 ②（文件头注 `TypeArgInference.z42:9`：「同一型参绑到两个不同类型 → 整体失败
  （不做 C# 式最佳公共类型）」）。本 change 把它补上。
- **#546（398c34d8）与本条正交**：它归一「已推出的**单个**绑定」以满足接口约束（`_resolvedForm`），
  不碰冲突路径。核实过，不重叠。

## What Changes（一个函数）

`TypeArgInference._unify` 的 `Z42GenericParamType` 冲突分支：同一型参绑到两个**数值**类型时，取
**算术拓宽公共类型**（`double > float > long > int`）而非整体失败。复用编译器既有的
`TypeFacts.ArithmeticResult`（`BinaryTypeTable.z42:92`，二元 `int + long` 用的同一张表），
数值判定用 `TypeFacts.IsNumeric`。

```
if (bound[idx] == null) { bound[idx] = a; return true; }
if (bound[idx].CanonName() == a.CanonName()) { return true; }
// best-common-type：双方都是数值 → 取拓宽公共类型（复用二元算术同表），写回后继续
if (TypeFacts.IsNumeric(bound[idx]) && TypeFacts.IsNumeric(a)) {
    bound[idx] = TypeFacts.ArithmeticResult(bound[idx], a);
    return true;
}
return false;   // 边界 ②：无数值公共类型 → 整体失败（保持静默，见 D2）
```

`Infer` 出口的 `_resolvedForm`（#546 加）已把结果 `Builtin("long")` 归一成 `Std.Int64` —— 无需改动。
3+ 参的折叠与顺序无关（数值拓宽是格上的 max）。

## 设计裁决（两条，User 确认）

- **D1 最佳公共类型规则 = 复用 `ArithmeticResult`（数值拓宽），v1 仅数值。**
  与语言里二元运算符的拓宽语义一致、零新概念；爆炸半径 ~0（所有数值输入都能转到拓宽后的类型，
  没有实参会因此新失败）。引用类型的公共基类（C# 式）是更大的独立设计 → 不在本轮。
- **D2 无数值公共类型的冲突（如 `Max("s", 7)` / int+uint 无拓宽）→ v1 仍静默失败。**
  字节不变、符合 #536 哲学；把它改成响亮的 E0402（「无法推断 T：实参冲突」）是独立的爆炸半径
  问题（可能把今天能编的代码变红）→ 拆独立 change。

## 净效果

更多混合数值泛型调用能编**并**被实参/约束校验；今天能编的一律不破。**无回灌 `MethodTypeArgs`**
（沿 #536 构造）⇒ 发射零改动、无格式 bump、自举字节不动点不受影响。

## Scope

- `src/compiler/z42c.semantics/src/TypeArgInference.z42`（`_unify` 冲突分支，~5 行）
- `src/compiler/z42c.semantics/tests/typecheck/generic_inference/generic_inference_tests.z42`（新增门）
- `docs/book/` 泛型推断机制页（补「数值冲突 → 拓宽公共类型」一段）

## 验证

- 阳性门（distinguishes 0→1）：`where T:IFoo` + `p(1, 2L)` → 修前推断失败不校验 = 0 个 E0402；
  修后 T 推成 long、`long` 不满足 IFoo → 1 个 E0402。证明推断确实成功并产出具体类型。
- 拓宽方向门：`p(1, 2L)`（无约束）→ 0 个 E0402（long 接受 int 与 long 两实参；若误合成 int 则 `2L`
  会因窄化报错 → 抓「合错方向」）。
- 保留 `test_conflicting_bindings_fail_inference_silently`（`p("s", 7)` string+int 无数值公共类型 →
  仍 0，D2 不变）。
- 完整 GREEN（含 `compiler` stage 跑 z42c.semantics [Test] 单元）+ 自举不动点。
