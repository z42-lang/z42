# Tasks: 把 `Self` 应用到核心协议接口

> 状态：🟢 完成（待归档）| 创建：2026-09-07 | 完成：2026-09-07
> 范围裁决（2026-09-07，User）：**三个接口全改**（`IEquatable` / `IComparable` / `INumber`）；
> bootstrap 锁步问题**先做 spike 实测再定** → **实测不冲突，单 PR 落地**。

## 进度

- [x] 0. 前置门：确认 nightly 已含 #506 `Self` + #520 关联类型（`merge-base --is-ancestor` 实测）
- [x] 1. 全仓 F-bounded 面测绘（3 接口 / 17 implements / 7 库约束 / 11 测试示例约束）
- [x] 2. baseline GREEN（未改动树）
- [x] 3. 接口声明 Self 化（3 个文件）+ `BuiltinTypeDefs.z42` 锁步
- [x] 4. 17 处 implements 子句去 `<T>`
- [x] 5. 7 处库内约束 + 11 处测试/示例约束去 `<T>`
- [x] 6. `generic_interface_dispatch.z42` 改用 `IEqualityComparer<int>` 承载原测试意图
- [x] 7. 全部验证门（见下）
- [x] 8. 文档同步

## 验证结果（全绿）

| 门 | 结果 |
|---|---|
| `build stdlib` | ✅ 25/25 succeeded |
| `build compiler`（z42c 自建） | ✅ |
| **`xtask test`（完整 GREEN）** | ✅ **0 failed**，2m54s |
| z42c self-host 不动点 | ✅ **3/3 packages gen1==gen2** |
| `test stdlib --mode jit` | ✅ **331/331 files**（23 libs） |
| `test e2e --dir cross-zpkg --mode jit` | ✅ |
| **`test bootstrap`** | ✅ **NO staged-bootstrap boundary violation** |

## ⭐ 实施结论

**① bootstrap 锁步的担心是多余的，但表仍须改。** `BuiltinTypeDefs` 是**导入侧 prelude 兜底**
（供「引用了这些接口但没加载 z42.core」的单元用）：编译 z42.core 自身时源码声明才是权威，
编译下游包时读的是真实 zpkg 元数据。故旧表 × 新声明不打架 —— spike（旧编译器编新 stdlib，
25/25）与 `test bootstrap`（真实 nightly）双证。

**② 零编译器代码改动。** 除 `BuiltinTypeDefs` 那张数据表外，语义/绑定/发射**一行没动**。
原因见 add-associated-types PR-2 的结论：`Self` 被实现成「接口的隐式类型参数」，与真型参 `T`
走同一条路，所以约束位、签名匹配、派发键三处天然零改动。

**③ `INumber` 的返回类型是 `Self` 但运算符派发不受影响** —— `ExprTyper._bindBinary` 的约束
派发分支**刻意不读** `gopMs.Signature.Ret`，结果类型恒取左操作数型参（`INumber` 抬头的
「T + T → T only」协议）。这条既有注释救了本次改写：否则 `Self` 会顺着返回类型漏进泛型代码。

**④ 唯一的表达力损失有专职接口兜底**：`class MyInt : IEquatable<int>`（异型相等）不再可表达，
全仓仅 1 处测试在用，已改写为 `IEqualityComparer<int>`（其 `T` 是被比较对象、**保持泛型**）。

## ⚠️ 顺带发现（未修，值得独立立项）

**z42c 完全不校验「类是否实现了接口成员」。** 实测探针：

```z42
interface I { void M(); }
class C : I { }                             // 缺 M —— 不报错
interface J { void N(Self s); }
class D : J { public void N(int x) { } }    // 签名与 Self 不符 —— 不报错
```

两条都静默通过。本次改写**受益于**这个洞（基元写 `Equals(int)` 而 `Self`=`Int32`，不被拒），
但它与 [[restore-emit-zbc-diagnostics-program]] 那条线的「binder 不认 / emitter 照发」同族。
`generic-constraints.md` 早已记着「今天没有类实现接口时的成员签名齐备性校验」，本次实证坐实。

## 边界（实测确认，未越界）

**未动**：`IComparer<T>` / `IEqualityComparer<T>` / `IEnumerable<T>` / `IEnumerator<T>` /
`IBasicCollection<T>` / `ISubscription<TD>` —— 它们的 `T` 是元素/比较对象，不是「实现方自己」。

**未动** `z42c.syntax/tests/decl.z42:207-208,236-237` —— parser 测试的字符串字面量（测「泛型
约束怎么解析」），与 stdlib 语义无关，改了反而削弱 parser 覆盖。
