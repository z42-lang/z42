# tasks：add-reflection-value-equality

状态：🟢 已完成（2026-09-25）

## 代码
- 🟢 `Std.Type`：`op_Equality` / `op_Inequality` / `Equals(object)` / `GetHashCode`，判据 `FullName`。
- 🟢 `Std.Reflection.MethodInfo`：同款，判据 `__qualified`（与 `Type` **必须一起做**，
  否则正好制造 `methodof.md` 警告的那种不对称）。
- 🟢 🔴 判空一律 `Object.ReferenceEquals` —— 运算符体内写 `a == null` 会**递归派发回自己**、栈溢出。

## 验证
- 🟢 七格全对：`typeof(int)==typeof(int)` / `!=` / `.Equals` / `x.GetType()==typeof(int)` /
  null 两侧 / `methodof` 对称。
- 🟢 **方法级 `typeof(U)==typeof(int)` 由此变 `true`** —— 这一刀就把方法级整条路修通了。
- 🟢 **阴性对照**：退回 stdlib 改动重建 → `methodof_basic` 判红；恢复 → 绿。
- 🟢 `xtask test all` / `examples` / `docs` / `lines` / `diagcodes` / `walkers` 全部 exit 0。
- 🟢 **爆炸半径 = 1 个用例**（`methodof_basic` 第 ⑥ 条），而它钉的正是被修掉的错误行为。

## 文档（两处规范冲突归一）
- 🟢 `methodof.md`：「对象身份」一行拆成**相等**（值相等，`true`）+ **对象身份**
  （不驻留，`ReferenceEquals` 为假）两行，并 📜 说明原归因（「没做驻留」）是错的。
- 🟢 `Type.z42` 头注：「is the *same* Type as」→「**equals**」+ 📜 说明它在本 change 之前是假断言。

## ⭐ 记下来的四条
- ⭐⭐ **坑点 ③ 的归因是错的**：它把「类级 typeof 产占位名」与「Type 无值相等」记成一条。
  实测后者**与泛型无关**（`typeof(int)==typeof(int)` 就已经是 false），且是招牌症状的真因 ——
  **只修 ③ 不会让症状消失**。修完本 change，类级只剩 `FullName` 还是 `"T"` 这一个真缺陷。
- ⭐⭐ 记忆里「方法级 `typeof(U)` 是对的」**只对了一半**：名字对、比较错。
- ⭐⭐ **「不驻留」不等于「== 为假」**。`methodof.md` 把对象身份与值相等混为一谈，并据此把一个
  bug 写成了刻意设计。C# 同样不驻留，但 `==` 为真。
- 🔴 **运算符体内判空不能用 `==`**（递归派发）。另记：`Object.ReferenceEquals` 声明了 public，
  但**用户代码调不到**（`_seedObjectStub` 只种了四个方法）—— 独立缺口，未修。
