# Tasks: fix-arity-mangle-package-wide

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：fix

**变更说明：** arity-mangle 的判据此前是 **per-CU** 的，而符号表是 per-package ⇒
「a.z42 声明 `class Foo`、b.z42 声明 `class Foo<T>`」时两个类型撞同一个裸键、last-wins，
**一个类型连同它的成员一起消失**。

## 同源对照（这条 bug 的判据）

同一份程序，**只差这两个类是否写在同一个文件里**：

| | 同一文件 | 拆两文件（修前） |
|---|---|---|
| `g.GetType().Name`（`g = new Foo<int>()`） | `Foo$1` | `Foo` |
| `g is Foo`（非泛型） | `false` ✅ | **`true`** ❌ |
| `p.N = 7`（`p = new Foo()`） | `7` ✅ | **运行期崩** `VCall: expected object, got Null` ❌ |

即**同一份源码，拆不拆文件决定它对不对**。修后跨文件输出与同文件**逐行一致**。

## 根因

判据在两处各算一份、且都只扫**当前 CU**：
- `StubCollector._passClassStubs` 开头（决定 `SymbolTable.Classes` 的键）
- `ExportedTypeExtractor._extractCore` 开头（决定 TSIG / 导出元数据的键）

而 `table.Classes` 是 per-package 的（`SymbolCollector.CollectAll` 把全部 CU 灌进同一张表）。
两个类各自所在的 CU 都「只有一个 arity」⇒ 都不 mangle ⇒ 撞键。

## 修复

`SymbolTable` 新增包级 `MultiArityNames` + `NoteArities(cu)`；`SymbolCollector` 在**任何**
`_passClassStubs` 之前对全部 CU 喂一遍（单 CU 路径喂那一个，结果与修前逐字相同），
两个生产点改读同一份集合。

🔴 **两处必须一起改**：只改符号表侧会让生产端（TSIG 键）与消费端（符号表键）对同一个类
算出不同的名字。

## 我差点把这条 Deferred 当成没证据而放弃

它是我在 `report-duplicate-type-name` 里**读代码**推出来登记的。这次动手前先做复现，
头几个探针（`new Foo<int>()` 调泛型方法、两个 arity 各调各的方法）**全都正常**——
因为发射端按 `<短类名>.<方法名>` 组合键在运行期照样能找到函数（正是
`restore-emit-zbc-diagnostics` 一路记录的那个 binder/emitter 不对称在「帮忙」）。
直到探到**类型身份**（`is` / `GetType`）与**字段布局**（`p.N = 7`）才炸出来。

⭐ 教训：**「调用能跑通」不等于「类型身份正确」**——查类型系统的 bug 要探身份与布局，
别只探调用。

## 🔴 第一版的门是**弱的**，退回对照当场戳穿

我最初只写了「非泛型在前、泛型在后」一种顺序的用例。做退回对照（把预扫改回逐 CU）时
**4 条全绿** —— 门没响。原因：判据累积在**表级**，这个顺序下轮到泛型那个 CU 时冲突已经被
记下了，所以碰巧仍然 mangle 对。真正会坏的是**反过来**：泛型先注册时判据还是空的、拿裸键，
随后被非泛型覆盖。补了反向顺序的两条用例后才成为真门。

⭐ 教训：**退回对照不只是"验一下"，它会告诉你门到底守没守住那件事**。这次它抓的不是实现，
是我的用例覆盖面。

## 验证

- [x] 端到端：修后跨文件输出与同文件**逐行一致**（`Foo$1` / `is` = false / `p.N=7`）
- [x] 单测 4 条（`collect_tests.z42`）：跨 CU 两个 arity 各拿到独立键 + **成员都还在**；
      同 CU 行为不变；单 arity 不过度 mangle
- [x] 退回对照：改回 per-CU 后差分用例变红
- [x] **自举不动点 3/3 —— 零字节漂移**（全仓「同包跨文件同名不同 arity」实测 0 组，键完全没变）
- [x] `xtask test` 全绿
