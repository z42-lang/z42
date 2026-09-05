# Proposal: 泛型约束表达力 —— 关联类型与 `Self`（参考 Rust / C# 截长补短）

> 类型：lang（完整流程：DRAFT → User 确认 → IMPL → GREEN → COMMIT）
> 创建：2026-09-05

## Why

User 要求：「参考 Rust 和 C# 的泛型约束，截长补短，吸取优秀的设计」。

前置已完成：change `complete-where-constraints`（#475 / #478 / #482）把**校验**补齐——编译期与
运行期 `validate_type_arg_constraint` 的七项对齐，27 条负例单测守着。那轮**完全没动表达力**，
所以现在是谈「加什么」的干净基线。

## 先摆证据：全仓真实约束只有 4 条，且形态高度集中

不靠印象，实测（`grep` 全仓 `src/libraries` + `src/compiler`，去掉测试与注释噪声）：

| 约束 | 出现次数 | 位置 |
|---|---|---|
| `where TKey : IEquatable<TKey>` | 2 | `Dictionary` / `DictionaryEnumerator` |
| `where T : IEquatable<T>` | 2 | 泛型集合辅助 |
| `interface INumber<T> where T : INumber<T>` | 1 | `Protocols/INumber.z42`（声明处） |

**4/4 条真实约束（连同 INumber 的声明）全是同一形态：`X<T> where T : Something<T>`。**

再查关联类型的**直接受益形态**——「双型参、其一可由另一推出」：全仓只有
`Dictionary<TKey, TValue>`，而它的 `TValue` 是**真正独立**的，推不出来。

⇒ **关联类型今天在 z42 有零个真实受益点；而自引用（F-bounded）形态占了 100%。**

## 这个证据指向的结论与 User 预期不同，必须先讲清

User 点名要推进的是**关联类型**。但证据显示：**z42 当前最该从 Rust 借的是 `Self` 类型，
不是关联类型。**

`X<T> where T : Something<T>` 这个形态在 Rust 里根本不存在——Rust 用 `Self`：

```z42
// 今天（C# 11 的 TSelf 模式，z42 照搬）
public interface INumber<T> where T : INumber<T> {
    static abstract T op_Add(T a, T b);
}
public class Int32 : INumber<Int32> { ... }
class Dictionary<TKey, TValue> where TKey : IEquatable<TKey> { ... }

// 有 Self 之后
public interface INumber {
    static abstract Self op_Add(Self a, Self b);
}
public class Int32 : INumber { ... }
class Dictionary<TKey, TValue> where TKey : IEquatable { ... }
```

`Self` 的三重收益，每一条都命中 z42 已知的**具体**问题：

1. **消掉 F-bounded 样板**——4 条真实约束全部简化，`INumber` 的 5 个 `static abstract` 签名同理。
2. **顺带关掉一条 Deferred**：`where-constraint-future-type-arg-matching`（接口约束只比裸名 →
   `IEquatable<string>` 也满足 `where T : IEquatable<T>`）。有了 `Self`，主流用法**根本不写类型
   实参**，这个 bug 类别在语法层就消失了——比实现「类型实参精确匹配算法」（要处理 F-bounded
   递归）便宜一个数量级，且效果更彻底。
3. **让约束可读**：`where TKey : IEquatable` 比 `where TKey : IEquatable<TKey>` 少一次心智绕行。

而关联类型的三个作用，逐条对照 z42 现状：

| 作用 | z42 现状 | 是否兑现 |
|---|---|---|
| 消掉到处传的类型参数 | 需要「泛型算法 over IEnumerable」这类深链；**LINQ 不存在**，全仓零个双型参可推形态 | ❌ 兑现不了 |
| 表达「结果类型由实现决定」（`Add::Output`）| 真需求，但 `INumber` 现在用 `static abstract T op_Add(T,T)`——**有了 `Self` 就够**，`Output` 要等出现异构运算（`Duration+Instant→Instant`）才需要 | ⚠️ 尚未到 |
| 防止同一类型实现两次 | 是正确性收益，但**被裸名匹配这个更底层的洞盖住** | ⚠️ 被遮蔽 |

> 补一条我在探索中**自我纠正**的事实：`foreach` **已经**支持枚举器协议
> （`StmtBinder._bindForeach` 三-path 决策树第三路，`add-foreach-ienumerable`，脱糖成
> `MoveNext`/`Current`/`Dispose`）。`IEnumerable.z42` 里「foreach codegen 升级是独立后续工作」
> 的注释**已过时**。但该路径是**鸭子类型**的（查 `Methods.ContainsKey("GetEnumerator")`），
> 不经过接口、元素类型靠 `var` 推断 → 依然吃不到关联类型。

## What（三个候选，请 User 裁决取舍）

### 候选 A —— `Self` 类型（**推荐先做**）

接口内可写 `Self` 指代「实现该接口的具体类型」；`class Int32 : INumber` 时 `Self` 绑到 `Int32`。
约束侧 `where T : IEquatable` 自动等价于今天的 `where T : IEquatable<T>`。

- **收益**：命中 100% 的真实用例；顺带关掉一条 Deferred。
- **代价**：中。parser 加 `Self` 类型名；符号表在接口内绑 `Self`；实现类接入时替换；
  **无需新语法结构**（`Self` 只是一个类型名，不是新的 where 子句形式）。

### 候选 B —— 关联类型 `type Item;` + `where T : IEnumerable<Item = U>`

- **收益**：今天为零；要等 LINQ / 异构运算出现才兑现。
- **代价**：高。parser（接口成员 `type X;` + where 里 `Name = Type`）→ 符号表（关联类型槽 +
  解析）→ 约束模型 → **zbc 格式**（绑定要跨包持久化）→ 运行期反射。
  跨 zbc 格式 = 走两-nightly 自举纪律。

### 候选 C —— C# 侧可借的小件

`notnull` / `unmanaged` / `default(T)` 约束。z42 无 nullable 类型体系（见 memory
`nullable-type-design-deliberation`，User 暂缓）→ `notnull` 无从谈起；`unmanaged` 依赖 blob
布局判定，z42 已有 struct 值语义可承载，但**无真实用例**。

## 推荐

**A → （观察）→ B**。理由：A 命中全部真实用例且顺带消掉一条债；B 的收益依赖 LINQ 与异构运算
这两个尚不存在的前提，先做等于建一个没有用户、且**没有跨包校验兜底**的特性
（跨包约束今天 100% 不校验，见 `complete-where-constraints` design §6）。

若 User 仍要求先做 B，需一并接受：本轮它**没有可应用的真实场景**，只能落 support + 测试用例，
「应用到需要的地方」要等 LINQ 立项。

## 🔴 硬约束：support 与 use 必须跨两个 nightly

无论选 A 还是 B，**新语法一旦落地，z42c 源 / xtask 源不得在同一 release 使用它**
（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 铁律）——否则上一版 nightly
编不了当前 main，跨版本自举断链。

⇒ **User 说的「对目前需要的地方进行应用」必须拆成两个 release**：
本轮只落 support（+ 测试用例，测试由自建 z42c 编译，不受此约束）；改写 `INumber` /
`Dictionary` 等**真实源码**要等新 nightly 发布之后。这不是保守，是自举链的硬机制。

**待确认**：stdlib 各库受此约束的**精确边界**需在 design 阶段实测确定——`z42.ir` 因是 z42c
运行期自依赖（bootstrap-seed 轴 ④）必然受限；其余库是否可立即使用需验证冷启动路径。

## Out of Scope

- 不做 nullable 类型体系（`notnull` 的前提，User 已暂缓）
- 不做 LINQ / iterator chain（B 的收益前提，独立立项）
- 不动跨包约束持久化（`complete-where-constraints` 的 Deferred，独立立项）

## Open Questions

- [ ] **选 A、B，还是 A+B？**（推荐 A 先行）
- [ ] `Self` 在**类**里是否也可用（Rust 可，C# 无）？还是仅限接口？
- [ ] 若选 B：关联类型的绑定由实现方**显式声明**（`type Item = T;`）还是从方法签名**推断**？
- [ ] 若选 B：zbc 格式 bump 的时机（需与其它 format 变更合流以省一轮两代自举）
