# Proposal: 把 `Self` 应用到核心协议接口 —— 消掉 F-bounded 样板

> 类型：lang（stdlib 公开 API 破坏性变更）｜ 完整流程：DRAFT → User 确认 → IMPL → GREEN → COMMIT
> 创建：2026-09-07

## Why

前置 change `add-associated-types` 交付了 `Self`（#506）与同包关联类型（#520），但**只落 support、
没落 use**——那是 [bootstrap-seed 轴①](../../../../.claude/rules/bootstrap-seed.md) 的纪律要求：
新语法必须晚一个 nightly 才能被自己的源码使用。

User 的原始需求「**并对目前需要的地方进行应用**」指的就是这一步。本 change 兑现它。

**前置门已确认满足**：nightly tag 指向 `b51400a4`，而 `7e0030bd`（#506 `Self`）与 `bf36a547`
（#520 关联类型）都是它的祖先（`git merge-base --is-ancestor` 双向实测）⇒ 上一版已发布 nightly
的 z42c **已具备** `Self` 能力，当前源码可以开始使用它。

## 先摆证据：全仓 F-bounded 接口只有 3 个

不靠印象，全仓实测（`src/libraries` + `src/compiler` + `src/tests` + `examples` + `scripts`）：

| 接口 | 声明位置 | 是否显式 F-bounded |
|---|---|---|
| `IEquatable<T>` | `Protocols/IEquatable.z42:6` | 隐式（无 where，但 `T` 语义就是「实现方自己」） |
| `IComparable<T>` | `Protocols/IComparable.z42:5` | 隐式 |
| `INumber<T>` | `Protocols/INumber.z42:13` | **显式**：`where T : INumber<T>` |

**明确不在范围内**（`T` 是元素/比较对象类型，不是「实现方自己」，改了就是错）：
`IComparer<T>` / `IEqualityComparer<T>` / `IEnumerable<T>` / `IEnumerator<T>` /
`IBasicCollection<T>` / `ISubscription<TD>`。

改写面（实测计数）：

| 类别 | 数量 | 位置 |
|---|---|---|
| 接口声明 | 3 | `z42.core/src/Protocols/` |
| implements 子句 | 17 | 13 × `z42.core`（11 基元 + `Char`/`Boolean` + `String`）+ 4 × 库测试 |
| 库内约束点 | 7 | `Dictionary` / `DictionaryEnumerator` / `HashSet` / `SortedSet`(×2) / `PriorityQueue` / `LinkedList` |
| 测试 + 示例约束点 | 11 | `src/tests/{generics,operators}` + `examples/generics.z42` |
| 编译器 prelude 表 | 1 | `BuiltinTypeDefs.z42:29-79`（**必须同步**，见下） |

## What Changes

```z42
// 前
public interface IEquatable<T> { bool Equals(T other); int GetHashCode(); }
public struct Int32 : IComparable<int>, IEquatable<int>, INumber<int> { … }
public class Dictionary<TKey, TValue> where TKey: IEquatable<TKey> { … }
public class SortedSet<T> : IBasicCollection<T> where T: IComparable<T> + IEquatable<T> { … }

// 后
public interface IEquatable { bool Equals(Self other); int GetHashCode(); }
public struct Int32 : IComparable, IEquatable, INumber { … }
public class Dictionary<TKey, TValue> where TKey: IEquatable { … }
public class SortedSet<T> : IBasicCollection<T> where T: IComparable + IEquatable { … }
```

## 代价：失去「与别的类型比较」的能力（User 已裁决接受）

`Self` 化后 `class MyInt : IEquatable<int>`（把 `MyInt` 与 `int` 比）**不再可表达**。

- 全仓仅 **1 处**在用：`src/tests/generics/generic_interface_dispatch.z42:4,15`（测试，非真实代码）。
- 该能力本就有**两个专职接口**覆盖：`IComparer<T>` / `IEqualityComparer<T>`（外部比较器形态），
  两者的抬头注释（`IComparer.z42:5-8` / `IEqualityComparer.z42:5-7`）已明写与 `IComparable`
  「自比较」形态的分工。
- 与 Rust 的 `Ord`/`PartialOrd`（`Self` 化）vs 显式比较器的分工一致。

⇒ 该测试改用 `IEqualityComparer<int>` 承载「泛型接口 TypeArgs 在方法参数上替换」这一原意，
覆盖不丢。

## 🔴 最大未知：BuiltinTypeDefs 与冷启动的锁步

`BuiltinTypeDefs.z42:29-79` 把 11 个 prelude 接口的形态**硬编码**在编译器里
（`_iface("IEquatable", _t1("T"), 1, m, 2)`，即 tpc=1、签名串 `"T"`），注释写明
「字节形态 = C# 同源实测」。本 change 必须把其中 3 个改成 tpc=0 + `"Self"`。

风险在冷启动：CI `ci-bootstrap` 是拿**上一版 nightly 的 z42c**（其内建表仍是 tpc=1）去编
**新版 z42.core 源码**（声明已是 `interface IEquatable { … }`）。这一步会不会冲突，**只能实测**。

**User 裁决：先做 spike 实测再定**（不预先按两-nightly 拆）。

### ✅ 实测结论：不冲突，单 PR 落地

两个独立证据：

1. **spike**：用**改动前**建好的 in-tree z42c（其 `BuiltinTypeDefs` 仍是 tpc=1）去编**改动后**的
   z42.core 源码 —— `build stdlib` **25/25 succeeded**。这正是冷启动的形状（旧表编译器 × 新声明源码）。
2. **`xtask test bootstrap`**（权威门，下载真实 nightly SDK）：

   ```
   ✅ nightly z42c compiles current source — NO staged-bootstrap boundary violation
   ✅ repo z42c self-build OK
   ```

**为什么不冲突**（机制解释）：`BuiltinTypeDefs` 那张表是**导入侧的 prelude 兜底**——供「引用了
这些接口但没加载 z42.core」的编译单元使用。编译 z42.core **自身**时，源码声明才是权威；编译
下游包时读的是新建出来的**真实 zpkg 元数据**，也不是这张表。因此表与声明短暂不同步不会打架。
表仍须改（本 PR 已改），否则那条兜底路径会给出过时形态。

## 不在本轮

- **关联类型的 use 改写**：全仓实测**零个真实受益点**（唯一双型参 `Dictionary<TKey,TValue>` 的
  `TValue` 真正独立、推不出来）。且跨包关联类型仍是 Deferred（`assoc-type-crosspkg`，需 zbc/zpkg
  双 minor bump）。本轮只用 `Self`。
- `self-return-type-substitution`（Deferred）：`IClone c; c.Copy()` 仍得到型参 `Self` 本身。
  本轮改写的三个接口里，`IComparable.CompareTo` 返回 `int`、`IEquatable.Equals` 返回 `bool`，
  只有 `INumber.op_*` 返回 `Self`——而运算符派发路径**刻意不读** `gopMs.Signature.Ret`
  （`ExprTyper.z42` 注释写明，结果类型恒取左操作数型参），故不受影响。

## 验证口径

- 完整 GREEN（`xtask test`）+ `test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit`
  （本地 GREEN 只跑 interp）。
- **`xtask test bootstrap`**：这是本 change 的**关键门**——它直接回答「上一版 nightly 能不能编
  当前源码」，也就是上面那个最大未知。
- 自举字节不动点（gen1 == gen2）。
