# Tasks: add-expression-bodied-members

> 状态：🟢 已完成 | 创建：2026-09-13 | 类型：lang（纯 parser 脱糖，零语义/IR/格式改动）

**变更说明：** 表达式体成员补齐到属性与索引器——`T P => e;`、`T P { get => e; }`、
`T this[..] => e;`、索引器访问器 `get => e;` / `set => e;`。此前只有表达式体**方法**
`int F() => 1;` 可用；属性写 `=>` 落进 `MemberParser` 的字段分支 ⇒ `E0202 expected ';'` 连环 +
`E0443 undefined type: =>`。

## 设计：纯 parser 脱糖

- `T P => e;` / `T P { get => e; }` 直接产出与 `T P { get { return e; } }` **逐字相同**的
  `PropertyDecl`（`HasGetBody` + 单语句 `return` 块）。下游符号收集 / 体绑定 / IrGen / 跨包导出
  **没有任何新路径**——parser 单测以 AST dump 相等断言。
- 方法 / 属性 / 索引器共用 `_parseArrowBody(isVoid)`（原方法尾里的内联实现提取而来）：非 void →
  `{ return e; }`；void（索引器 `set`）→ `{ e; }`。
- 方法与属性靠成员名后的 token 区分：`(` → 方法，`=>` → 属性（泛型方法的 `<T>` 在此之前已消费）。
- 无 zbc / zpkg 格式改动 ⇒ 无 version bump；新语法只被测试使用，z42c / stdlib / xtask 源码未使用
  ⇒ 符合 bootstrap-seed 的「support 先行、晚一 nightly 再 use」。

## 事实校正（立项时的判断有误）

迭代清单曾记「本特性解锁 `examples/generics.z42`、`examples/oop.z42`」。实测**不成立**：两者
还卡着其它 C#-ism（见 `scripts/test/examples-known-broken.txt` 已更新的注释）——
generics：空容忍后缀 `x!`、`Option<int>.Some(..)`、`new[] {..}`；oop：`return default;`、插值格式
说明 `{x:F2}`、三元里的 target-typed `new(..)`。本 change 后两文件**不再报 `=>` 相关诊断**，但仍编不过
⇒ 名单保留（双向棘轮不受影响）。

## 发现的既有 bug（未在本 change 修；User 裁决 2026-09-13：另开 change 实现静态属性支持）

**静态属性整体静默错编**（任何写法，块体 `static int K { get { return 42; } }` 同样中招，与本 change 无关）：
- 使用位 `St.K` 发 `static_get Demo.St.K`（不存在的静态字段）⇒ 运行期 `MissingSymbolException`，编译期零诊断；
- 静态 auto 属性 `get_A` 是 0 参静态函数却发 `field_get %0.__prop_A`；`St.A = 5` 发成对不存在字段的 `static_set`；
- 类内裸名 `K` 报 `E0401 undefined`。
- 全仓（`src/` `examples/` `scripts/`）**零处**声明静态属性 ⇒ 从没人走过。

## 验证

- [x] parser 单测 `z42c.syntax/tests/decl.z42`：`test_expression_bodied_property`（AST 与块体逐字相等）+ `test_expression_bodied_indexer`
- [x] 行为测试 `src/tests/types/expression_bodied_members.z42`：抽象/接口/虚属性 override、类内裸名、泛型类、struct、`get =>`、索引器 `=>` 与 `get/set =>`；interp + JIT 均过
- [x] 退回对照：种子（nightly）编译器编该测试报 47 条 E0202；改坏一条断言 ⇒ `TestFailure`
- [x] GREEN（`xtask test` 全 stage，GREEN_EXIT=0，自举不动点 3/3；基于 main `7c6eefaed`，rebase 后冷重跑）
