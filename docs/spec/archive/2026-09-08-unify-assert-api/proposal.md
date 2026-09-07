# Proposal: 合并两份 Assert —— 消掉 stdlib 唯一的跨命名空间同短名类

> 类型：stdlib（删除一个 public 类，合并 API 面）
> 归属程序：[[restore-emit-zbc-diagnostics-program]] 的前置项（见「为什么现在做」）
> 状态：DRAFT → User 已在会话中确认方向（合并；统一那份住 z42.test，`TestFailure`/`SkipSignal` 原地不动）

## 问题

仓里有两个 `Assert` 类，短名相同、命名空间不同：

| 类 | 包 | 方法数 | 失败时抛 |
|---|---|---|---|
| `Std.Assert` | z42.core（prelude） | 8 | `Exception("AssertionError: …")` |
| `Std.Test.Assert` | z42.test | 23 | `TestFailure` / `SkipSignal`（结构化） |

**这是整个 stdlib 唯一的跨命名空间同短名类**（全仓扫描 `src/libraries/*/src/**.z42` 的所有
class/struct/interface，按短名分组，只有 `Assert` 落在 ≥2 个 ns 上）。

这一对类单独撑起了三处特判：

1. `.claude/rules/common-pitfalls.md` §1 的「加载顺序 first-wins」现场案例，正是
   `Std.Assert.Equal` vs `Std.Test.Assert.Equal` 抢同一个短键 `"Assert.Equal"`；
2. `CuPreprocess._activeNamespaces` 必须把 prelude ns 显式纳入活跃集，否则
   `using Std.Test` 会把裸 `Assert.Equal` 唯一命中到 `Std.Test.Assert`
   （即 common-pitfalls 记的「AssertionError → values not equal」回归）；
3. `DependencyIndex.GetStaticScoped` 的「活跃集内仍歧义 → 返回 null → 回落短键
   prelude-first」分支，存在的唯一理由就是这一对。

更严重的是它让 **binder 与 emitter 长期不一致**：

- **emitter** 按 `<短类名>.<方法名>` **组合键**解析 —— `Assert.Greater` 只有 z42.test 一条命中，
  于是正确绑到 `Std.Test.Assert.Greater`；
- **binder** 先定类、再找成员 —— 裸名 `Assert` 经 `SymbolTable.GetClass` first-wins 定到
  `Std.Assert`，然后报 `E0401: no static method 'Greater' on 'Assert'`。

这条不对称被 `--emit-zbc` 吞诊断掩盖了很久（诊断没人读，emitter 那半边跑得通，测试就绿）。
实测：修前编出的 `.zbc` 跑起来 `assert_numeric_helpers` 25 passed / `assert_collection_helpers`
12 passed / `dogfood` 26 passed —— 全是 binder 声称「不存在」的那些方法。

## 为什么现在做

`restore-emit-zbc-diagnostics` 要给 `--emit-zbc` 装上「有诊断就非零退出」的门。装上后冷扫全部
641 个单文件语料，**216 条 E0401 全部来自这一对 Assert**（占全部诊断的 1/3、涉及约 40 个文件）。

不消掉这个碰撞，开门就只有两条路：要么把 ~274 个同时 `using Std;` 与 `using Std.Test;` 的文件
逐个加限定，要么给编译器塞一条只为这一对类存在的特判。合并是从根上拿掉起因。

## 方案

**唯一 `Assert` = `namespace Std`，打包在 z42.core。** 具体：

- 删除 z42.core 原来那份 8 方法的 `Assert.z42`；
- `src/libraries/z42.test/src/Assert.z42` **移动到** `src/libraries/z42.core/src/Assert.z42`，
  `namespace Std.Test;` 改成 `namespace Std;`（方法集不变，23 个；实现不变，一律抛
  `TestFailure` / `SkipSignal`）；
- `src/libraries/z42.test/src/Failure.z42`（`TestFailure` / `SkipSignal`）**一并移到 z42.core**。
  它本就是 `namespace Std`，移动的只是打包位置，FQN 不变。

两个被移动的文件都**只依赖 core 符号**（`Exception` / `Action` / 基元），搬包不引入任何新依赖。

### 为什么必须落在 z42.core（实施期实测推翻的一版设计）

中途试过「唯一那份留在 z42.test」的变体，理由是「全仓非测试代码没有一处真调 `Assert.`，
断言 API 不必进 prelude」。**这个变体被 `xtask test` 当场否掉：183 条 golden 全红，
`VCall: expected object, got Null`。**

根因：`ImportedSymbolLoader._isPrelude` 判的是**包名**——

```z42
private static bool _isPrelude(string pkg) { return pkg == "z42.core"; }
```

非 prelude 包必须由 `using` 命中它的某个 ns 才会进激活集。而 `src/tests/` 下有 **188 个 golden
一行 `using` 都不写**、纯靠 prelude 拿到 `Assert`。`Assert` 一旦离开 z42.core，这些文件的
`Assert` 就解析不到 → 落进实例路径 → 运行期 `VCall: expected object, got Null`
（编译期还 exit 0 照写产物 —— 正是本程序在修的那个洞，它让这个错误在编译期完全无声）。

| 方案 | 改动 | 调用点 |
|---|---|---|
| **A（采纳）**：两个文件都搬进 z42.core | 移 2 个文件 | **0** |
| B：留 z42.test + 给 188 个 golden 补 `using Std;` | 改 188 个文件 | 188 |

⇒ 原设计「测试专属语义不应进 prelude」这条**在 z42 的 prelude 是包粒度**这个事实面前站不住：
断言就是 prelude 级设施，它抛的异常类型只能跟着一起进来。

### 调用点：零改动

600 个写裸 `Assert.` 的文件一处都不用改：

| using 组合 | 文件数 | 合并前 | 合并后 |
|---|---|---|---|
| 无 using | 188 | prelude → `Std.Assert`（8 方法版） | prelude → 唯一的 `Std.Assert`（23 方法版）✓ |
| 只 `using Std;` | 71 | `Std.Assert` | 同上 ✓ |
| 只 `using Std.Test;` | 68 | ❌ binder 绑到 `Std.Assert` 却报「no static method」（bug） | 同上 ✓ |
| 两个都有 | 274 | `Std.Assert`（歧义→prelude-first） | 同上 ✓ |

## 行为变化（可观测的只有两处）

1. **原 `Std.Assert` 那 8 个方法的失败异常类型**从裸 `Exception("AssertionError: …")` 变成
   `TestFailure`。这是**改善**：runner 靠异常类型区分「故意的断言失败」与「意外崩溃」，
   合并前用到 core 那份的地方（含 274 个"歧义→prelude-first"文件）拿到的都是裸 Exception，
   一律被报成「意外错误」。
2. `TestRunner.Fail` 从打印 `e.Message` 改成 `e.ToString()`。因为 `TestFailure.Message` 只是
   概括语（"values not equal"），actual/expected 在 `ToString()` 里；不改就是把
   `AssertionError: expected 1 but got 2` 降级成 `values not equal`。
   → `z42.test/tests/test_runner/expected_output.txt` 一个 golden 随之刷新。

## 不做

- **不改 `ArrayContains` / `ArrayIsEmpty` 等的 `Array` 前缀。** 那个前缀当初是为绕开同一个
  碰撞加的，合并后理由消失，但改名纯属调用点 churn，且前缀本身读起来更明确。留注释记录由来。
- **不修编译器。** 合并只是让本仓不再有语料能触发那条 binder/emitter 不对称，**不等于修好了它**。
  裸名解析（按活跃 ns 集 + 对标 C# 的歧义诊断）与配套的双-ns 同短名 fixture 负例门，
  留给 `restore-emit-zbc-diagnostics` 那条 change —— 否则这就是又一道「没有东西盯着」的空门。

## 自举/种子

**不踩 bootstrap-seed 轴②**（stdlib API 面）：xtask（`scripts/`）与 z42c 源只引用
`Std.Test.ModuleLoader` / `Std.Test.Runner` / `Std.Test.TestEntry`，**不碰 `Std.Test.Assert`**
（已 grep 全部引用点核实）。用到 Assert 的全是测试语料，由 in-tree 自建编译器编译，不受种子约束。
