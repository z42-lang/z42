# proposal：fix-path-dep-closure

## 一句话

`path` 依赖的**传递闭包**被构建了、却只有**直接依赖**被搬进 exe 的 dist ⇒ **依赖链深度 >1 就不可用**：
编译期一切正常，运行期 `MissingSymbolException`。

## 症状

```
app  → 声明 acme.web  { path = "../web" }
web  → 声明 acme.util { path = "../util" }
```

```
$ z42 run app/app.z42.toml
Error: uncaught exception: Std.MissingSymbolException: type `Acme.Util.Helper` could not be resolved
  at Acme.Web.Widget.V(Widget)
```

`app/dist/` 里只有 `acme.web.zpkg` + `demo.app.zpkg`，**没有 `acme.util.zpkg`**。
**决定性实验**：手工把它拷进去 → 立刻打出 `42`。

⚠️ 注意 `app` 的源码**从未引用过** `Acme.Util` 的任何符号 —— 是 `acme.web` 内部在用。
所以这既不是 E0497（未声明依赖）能覆盖的，也不是用户能靠「写全依赖」绕过的。

## 根因：一条理由站不住的 Deferred

`ExeDeps.z42` 头注白纸黑字：

> **仅直接依赖**（与 publish 一致：exe 应直接声明全部所需兄弟；**传递闭包记 Deferred**）

循环体是 `while (i < pm.DepCount)` —— 只遍历消费方**自己声明**的那几条。

**那个理由站不住**：`app` 根本没引用过 `baz` 的符号，要求消费方声明一个自己不用的包，
正是包管理器该替你做的事（Cargo / npm 都不要求你手工展平传递闭包）。

对照：`PathDepPlan.Resolve` **本来就算的是传递闭包**（「传递 + 去重 + 拓扑序」），
`_build` 也把闭包全体的 dist 并进了 `libsDirs` —— 所以**编译期一直是对的**，
只有产物装配这一步落下了。

## ⭐ 它藏了一个月的原因：e2e 只造了 1 层

`add-path-dependencies`（2026-08-29）阶段 2.5 的 e2e 是 `lib foo + exe bar`——**深度 1**。
而深度为 1 时「直接依赖」**恰好等于**「闭包」⇒ 缺陷被完整遮住，检查照常 ✓。

同族于 `static_abstract_operator` 挑中单字段 `Money` 恰好绕开 sret 那条 ——
**「测试写了但抓不到 bug」的又一个样本**。设计文档里还写着「path 闭包通常极小
（interactive→repl 仅 1 层）」，那句观察是对的，但它恰恰解释了为什么没人撞见。

## 改动（两处）

1. **`_bundleExeDeps`**：待拷名单从 `pm.Deps` 扩成 **`pm.Deps ∪ path 闭包全体`**（去重）。
   闭包成员恒为私有包（按路径引入、不在 `Z42_LIBS`）⇒ 一律拷。
2. **`_build`**：把闭包成员名抄出来传下去（`closureNames` / `closureN`）。

🔴 **不在 `_bundleExeDeps` 里就地重算闭包**（初稿这么写过）：非 top-level 子建
（`libsDirsCount > 0`）本就跳过闭包解析，那个语境下重算会抛
`No such file or directory` —— 被 e2e `buildclos` 当场抓下。

## 验证

- 深度 2 → `42`；深度 3（再加一层 `acme.base`）→ `45`，三个传递包全部进 dist。
- e2e `path 依赖` 检查**加深到 2 层**（新增 `baz`，`foo` 依赖它，`bar` **故意不声明**）。
- ⚠️ **阴性对照第一次是假绿**：`e2eDir` 在 `artifacts/.scratch/e2e` 下跨轮次保留，
  上一轮（带修复）留下的 `pathdep.baz.zpkg` 让「文件存在」判据照样通过。
  清掉才正确判红 ⇒ **fixture 现在自己先 `Directory.Delete(pdRoot, true)`**。
  **凡是「判据 = 文件存在与否」的检查，都必须自己把地面清干净。**
- `xtask test all` / `examples` / `dist` / `lines` / `docs` / `diagcodes` 全部 exit 0。

## 边界

- 只覆盖 **path 依赖闭包**。「workspace 兄弟包 A 依赖 B、exe 只声明 A」这一形状不在本次
  （workspace 走 `libsDirsOverride`、由 orchestrator 组装，机制不同）。
- publish 侧 `_pubCopyDistDeps` 从 dist 整体搬运，故自动受益于本修复。
