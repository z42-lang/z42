# Proposal: 包初始化失败的「归属判定」改成构造式

> 状态：**DRAFT，待 User 裁决**（vm 语义变更 ⇒ 规范先行）。
> 来源 = #785（`module-init-failure-not-catchable` 的修复）自己留下的洞。

## 1. 现状与缺陷（已实测确证，不是边界情况）

`[ModuleInit]` 失败后，「触达这个包的符号要重抛」靠
[`cctor.rs::module_covers`](../../../../src/runtime/src/vm_context/cctor.rs) 判定：

```rust
// "Ns.$Module" → 前缀 "Ns."；sym_fq.starts_with(prefix)
```

而 `$Module` 落在哪个命名空间，由**谁写了 `[ModuleInit]` 那个 CU 的 ns** 决定 ——
与「这个包拥有哪些符号」**没有关系**。该函数上游的文档注释自己也承认了这点（「⚠️ 已知边界」）。

两个方向都会错，用本仓库真实布局验过：

| 方向 | 形态 | 后果 |
|---|---|---|
| **漏判** | `z42.compression` 的 `[ModuleInit]` 写在 `namespace Std.Compression` | 失败后触达 `Std.Archive.ZipReader` **不重抛** ⇒ 在未初始化的包上继续跑。而 `z42.compression` 恰是最该用 `[ModuleInit]` 的那类包（参考手册的例子就是「打开 native 库」）|
| **过判** | 同一个包写在裸 `namespace Std`（每个库都有这样的 CU）| `$Module` = `Std.$Module`、前缀 `Std.` ⇒ 一失败**毒掉所有 `Std.*`**，包括 `Std.IO.Console.WriteLine` —— #785 刚修掉的 bug 缩小范围地回来 |

## 2. 🔴 新发现：「改成算 owner 命名空间前缀集合」也不成立

先前记录的修法是「从包的符号算出 **owner 前缀集合**」。**那个方案救不了过判方向**：

```
Std 这一个命名空间由 11 个包共同声明（z42.core / z42.io / z42.net / z42.compression / …），
而每个包都有直接落在裸 Std 下的符号。
```

⇒ 任何包的 owner 前缀集合都会包含 `Std.`，过判原样回来。**只要判据是「前缀」，就一定不精确**，
因为命名空间在 z42 里**本来就不是包的边界**（这正是 E0497 那条诊断反复强调的事：
「判据是类型的归属包，不是 using 的命名空间」）。

## 3. 提议：判据换成**成员归属**（构造式）

登记 `$Module` 时，把**这个包实际拥有的符号名集合**一起存下来；判定改为查集合。

- `sym_fq` 可能是三种形态（来自屏障的三类触达点）：类型 FQN / 静态字段 FQN / 函数 FQN。
- 判定：先查 `sym_fq` 本身；不中则**逐段剥掉末段**再查（覆盖 `Owner.Field` 与 `Owner.M$1`）。
- 不依赖任何命名约定 ⇒ 前缀问题整类消失。

### 3.1 为什么这次能做成「没有静默失效的路径」

`register_module_init` 目前从 `insert_type`（逐类型漏斗）调用，那里没有整包视野。
但查清了：**`insert_type` 的全部调用方都在整包循环里**

| 缝 | 位置 | 手上有什么 |
|---|---|---|
| 磁盘/内存 zpkg | `lazy_loader/registry.rs:105` | `artifact.module.functions` 循环刚跑完 + 正在遍历 `type_registry` |
| 第二条注册路径 | `lazy_loader/registry.rs:351` | 同上（待确认函数表是否同样在手）|
| 急切主包 | `lazy_loader.rs::seed_types_for_lookup`（绕开 `insert_type`）| 只有 types，**函数待确认** |

⇒ 把 owner 集合做成 `register_module_init` 的**必填参数**：任何漏供的路径**编译期就不过**，
不存在「忘了供 ⇒ 静默退回旧判据」这种形态（本仓库反复吃亏的正是这类）。

## 4. 待 User 裁决

1. **判据形态**：成员集合（本提案）／其他？
2. **`seed_types_for_lookup` 的函数表**：若急切主包这条路拿不到函数名，主包自己的**自由函数**
   触达就判不出归属。可选：① 该路径也传函数表（需查调用方手上有没有）；
   ② 主包特例「`$Module` 覆盖本包全部」（主包只有一个，语义上说得通）。
3. **内存开销**：只有声明了 `[ModuleInit]` 的包才存集合（今天全仓 0 个生产用例，stdlib 未用）
   ⇒ 实际近零。是否接受？
4. **要不要同时收紧诊断**：E0485 只保证「一个包至多一个初始化器」。是否顺带让
   「`[ModuleInit]` 写在一个不属于本包主要命名空间的 CU 里」不再是个需要判据去猜的事？
   （若判据改成成员集合，这条就**不需要**了 —— 列在这里是为了明确「不做」。）

## 5. 验收

- 单测 `cctor.rs::module_covers_only_its_own_namespace` 按新判据重写；
- e2e `src/tests/cross-zpkg/module_init_failure_catchable` 新增两格：
  **同包的兄弟命名空间要重抛**、**别的包不许被毒**；
- 判别力：把判据退回前缀 ⇒ 这两格必须分别红。
