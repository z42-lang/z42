# Proposal: 接口返回位的 struct 协变 —— 从静默崩溃到可用

> 状态：🔵 DRAFT 待审批 ｜ 类型：lang / vm（需规范先行）｜ 创建：2026-09-25

## Why

### 1. 现状是「编译期零诊断 + 运行期崩」

接口满足性**允许**「实现返回具体 struct / 接口声明返回引用型」的协变，但两侧**调用约定不一致**，
于是这段代码编译全绿、跑起来崩（干净 main `3757a052c` 实测）：

```z42
public interface IThing { int V(); }
[Record] public struct Two(int _a) : IThing { int _b; public int V() { return this._a; } }
public interface IBox { IThing Get(); }
public class Impl : IBox { public Two Get() { return new Two(11); } }   // 协变：Two 实现 IThing

// IBox b = new Impl(); b.Get();
//   编译：EXIT=0，零诊断
//   运行：Std.MissingSymbolException: `Impl.Get` resolved to a definition whose signature
//         does not match this call (it takes 2 physical argument(s), the call passes 1)
```

**判别边界**（逐条实测钉过）：

| 变量 | 结果 |
|---|---|
| 实现返回**一字段** struct（`SThing`）| ✅ 通过 —— 返回值走寄存器，无 sret |
| 实现返回**两字段** struct（`Two` / `ListEnumerator<T>`）| ❌ 运行期崩 |
| 泛型 | **不是**必要条件（非泛型同模块即可复现）|
| 跨包 | **不是**必要条件（同模块即可复现）|
| `foreach` | **不是**必要条件（直接经接口调一样崩）|

### 2. 根因：sret 由**调用点的静态返回类型**决定，接口调用点看不到实现的形状

- `CallEmitter.z42:172`：`bool retStruct = this._ee._isBlobStruct(c.Type())` —— `c.Type()` 是
  **调用点的静态返回类型**。
- 具体调用点：静态返回 `Two` = blob struct ⇒ 传隐藏 sret 槽，与被调方一致。
- 接口调用点：静态返回 `IThing` = 引用型 ⇒ **不传** sret 槽，而被调方（同一个 `Impl.Get`）
  是按 sret 编译的 ⇒ 物理实参数差一个。
- 放行点在 `InheritanceResolver.z42:481-484`：协变判定只问「`implRet` 是不是 `wantRet` 的
  子类 / 实现」（`table.IsSubclassOf` / `table.Implements`），**不问两者 ABI 是否一致**。

VM 侧那道门是**对的**：`symres.rs:195` 读 `METHOD_FLAG_SRET` 算
`expected = param_count + sret`，失配即抛 —— 它是 `fix-call-arity-skew`（zbc 1.40）刻意建的，
把一次静默错调变成了看得见的异常。**要修的是编译期放行了一个 ABI 不自洽的形状，不是那道门。**

### 3. 这条挡住了一个真实的用户可见能力

`List<T>` 至今**没有基表**（`Collections/List.z42:21` 是裸 `public partial class List<T> {`），
而 `Protocols/IEnumerable.z42:8` 的注释把 `class List<T> : IEnumerable<T>` 写成目标已久，
`ListEnumerator.z42` 也早已写好。实测加上两条基表后：

- `z42c build` / `xtask build stdlib` **全绿**（25/25，产物非空、比基线大 53 字节）；
- 直接 `foreach (v in list)` 正常（走索引快路径）；
- **经 `IEnumerable<int>` 静态类型迭代就崩**（`ListEnumerator<T>` 正好两字段：`_list` + `_pos`）。

⇒ 「`List<T>` 为什么没声明它实现了 `IEnumerable<T>`」的答案就是这条缺陷；注释里那句目标，
多半就是有人走到这里放弃的。修完它才谈得上 LINQ / 集合视图那一类。

### 4. z42 没有 C# 的逃生口

C# 用**显式接口实现**解决同一个矛盾：`List<T>` 同时有 `public Enumerator GetEnumerator()`
（pattern-based 无装箱）与 `IEnumerator<T> IEnumerable<T>.GetEnumerator()`（装箱、给接口用）。
z42 **没有显式接口实现语法**（全仓零命中），也就没有「同名两签名」的表达方式。

更关键的是：`CallEmitter.z42:168-171` 写明 **VCall 按 vtable slot(方法名)派发、arity 不入解析键**
⇒ **同名桥接方法无法与具体方法共存于同一个槽**。所以这不是「合成一个方法」就完事，
它要动**派发键的形状**或**sret 的协商方式**。

## What Changes

两件事一起做（User 2026-09-25 裁决「两条一起做」）：

1. **止血**：ABI 不自洽的协变不再静默放行 —— 在 `InheritanceResolver` 的协变判定处加 ABI 兼容
   要求，报新诊断（说清原因 + 给出可行写法）。**仅在桥接无法成立时生效**。
2. **可用**：让「实现返回具体 struct、接口声明返回引用型」这一形状**真能跑**，从而
   `ListEnumerator<T> : IEnumerator<T>` + `List<T> : IEnumerable<T>` 可以落地。

第 2 件的**落法有三种形状，取舍不同，见 [design.md](design.md) 的 D1** —— 需 User 裁决后才写码。

## Non-goals

- **不引入显式接口实现语法**（`IFace.Member` 形态）—— 那是独立的语言特性决策。
- **不改 `symres.rs` 那道 arity 门的判据**：它守的是真 skew，`fix-call-arity-skew` 的教训不推翻。
- **不动 `List<T>` 自身 foreach 的索引快路径**（Decision 8 无装箱前提不变）。
- **不做 LINQ / 集合视图**——本 change 只解锁前提。

## 相关

- `add-foreach-ienumerable`（归档）：Decision 8「pattern-based 无装箱」= `GetEnumerator` 返回
  具体 struct 的由来。
- #779 / #789 / #792：同一族（接口/泛型元数据在 bare-name 截断处丢信息）。
- `fix-call-arity-skew`（zbc 1.40）：sret 进 `method_flags bit3` + VM 侧 arity 门。
