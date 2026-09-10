# Tasks: fix-explicit-type-arg-not-substituted

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：fix

**变更说明：** 修复**显式写出类型实参时，签名没被代换就拿去绑实参** —— lambda 形参因此拿到裸 `T`：

```z42
Array.Sort<int>(xs, (a, b) => b - a);
//                        ^^^^^ E0402: operator `-` requires numeric operand, got `T`
```

## 根因（两层）

**第一层：代换发生得太晚。** `MemberResolver._applyMethodTypeArgs` 在 `_withDefaults` **之后**才跑，
注释里写得很清楚：「代换结果天然只能影响诊断、进不了 target-typed new / lambda 绑定 / 装箱决策」。
那条不变式是 `add-generic-type-arg-inference`(#536) 为**推断**路径定的（推断结果不回灌，design D4），
但它把**显式**写出的类型实参也一起挡在了外面 —— 而 `Sort<int>` 的签名本来就是
`Sort(int[], Comparison<int>)`，让实参照着它绑才是正确语义（C# 同）。

**第二层（真正卡住的那个）：`_substByName` 没有 `Z42FuncType` 分支。**
它只递归数组元素与实例化类型实参。而 lambda 形参类型**只**来自目标类型
（`BindArgsToSignature` → `BindWithTarget(rawArg, sig.ParamTypes[i])`），目标恰恰是个函数类型 ⇒
`Comparison<T>` 里的 `T` 换不掉 ⇒ 形参仍是裸 `T`。

> 🔴 `TypeArgInference._unify` 的注释把这个不对称记成「**这不会出错（只是少换一次）**」——
> 那句话是错的，全仓 18 条 E0402 全出自这条缺失分支。已在原地更正。
> 又一次印证：**没有东西盯着的断言迟早变成谎言**。

## 根因修复

- `MethodSymbol` 新增 `TypeParamNames`（方法级型参**名**，声明序）。`TypeParamCount` 只够做 arity
  过滤，按名代换必须有名字。本地取 `Decl.TypeParams.Names`；跨包取 `ExportedMethodZ.TypeParams`
  —— 那些名字 #523(R1) 起就已经读进来了，只是当解析上下文用完就丢，没留在符号上。
- `MethodSymbol.WithSignature`：换签名的浅拷贝。**放在 MethodSymbol 自己身上**，让「字段清单」与
  「拷贝清单」贴在一起 —— 调用方就地 `new` 会让新字段在某条路径上静默丢失（`RegKey` 丢了就发错函数名）。
- `MemberResolver._substForExplicitTypeArgs`：`call.TypeArgCount > 0` 且方法级型参 arity 相符时，
  按名代换整条签名（形参 + 返回），返回换了签名的 `ms`。arity 不符时原样返回 —— 那是 E0445 的活。
- `_substByName` 补 `Z42FuncType` 分支（形参 + 返回递归，`ParamsFrom`/`ParamDefaults`/`ParamCallers` 随类型走）。
- 7 个调用形态各接一次：自由函数 / 实例 / 接口 / 实例化 / 裸类名静态 / prim wrapper 静态 / ns 限定静态。

**推断路径原样不动** —— #536「不回灌推断结果」的裁决完整保留。两条不变式的分工写进了
`docs/book/src/language/generics.md`（对照表）。

## 代价（已知并接受）

lambda 形参类型进闭包签名 ⇒ **发射字节会变**，自举走两代收敛，**无格式 bump**（先例：#523）。
`RegKey` 保持原样 ⇒ 调用目标名不变。

## 验证

- [x] 最小复现 `Array.Sort<int>(xs, (a,b) => b - a)`：修前 2 条 E0402，修后编译干净 **且运行正确**
      （`[3,1,2]` 降序 → 首元素 3）
- [x] 三个受害文件（`array_algorithms` / `array_csharp_algorithms` / `array_range_overloads`）
      18 条 E0402 → **0**
- [x] `xtask test` 全绿 + 自举不动点
