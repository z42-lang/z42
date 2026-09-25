# tasks：fix-path-dep-closure

状态：🟢 已完成（2026-09-25）

## 代码
- 🟢 `ExeDeps._bundleExeDeps`：待拷名单 = `pm.Deps ∪ path 闭包全体`（去重）；签名加
  `closureNames` / `closureN`。
- 🟢 `Main._build`：闭包循环里抄出成员名，三个调用点一并传下去。
- 🟢 🔴 **不就地重算闭包**——非 top-level 子建会抛 `No such file or directory`（初稿踩过）。

## 测试
- 🟢 e2e `path 依赖` 检查**从 1 层加深到 2 层**（新增 `baz`；`foo` 依赖它；`bar` **故意不声明**
  ——它根本没引用过 baz 的符号）。判据从「foo.zpkg 在不在」扩成「foo + **baz** 都在」。
- 🟢 fixture **自清**（`Directory.Delete(pdRoot, true)`）。

## 验证
- 🟢 手工 fixture：深度 2 → `42`；深度 3 → `45`；三个传递包全部进 dist。
- 🟢 **阴性对照（第二次才是真的）**：退回修复 → `✗ 传递依赖 baz.zpkg 未 colocate`；恢复 → ✓。
  ⚠️ **第一次是假绿**：`e2eDir` 跨轮次保留，上一轮留下的 `baz.zpkg` 让「文件存在」判据照样过。
- 🟢 `xtask test all` / `examples` / **`dist`**（动的是产物装配，单列一跑）/ `lines` / `docs` /
  `diagcodes` 全部 exit 0。

## 文档
- 🟢 `internals/compiler/project-model.md` 第 3 步：「path 依赖的 zpkg」→「**闭包全体**」，
  补「名单由 `_build` 透下来、不在这里重算」+ 📜 说明旧行为与它藏一个月的原因。

## ⭐ 记下来的三条
- ⭐⭐ **「Deferred」不等于「这样做是对的」**。`ExeDeps.z42` 的头注给了理由（「exe 应直接声明
  全部所需兄弟」），而那个理由站不住——消费方**根本没引用过**传递包的符号。
  ⇒ 读到 Deferred，要问的是「它的理由今天还成立吗」，不是「有人想过了那就算了」。
- ⭐⭐ **深度 1 的 fixture 验不出「闭包 vs 直接依赖」**——两者在深度 1 恰好相等。
  凡是「传递/递归」语义，fixture **至少要 2 层**。
- ⭐⭐ **判据是「文件存在与否」的检查必须自清地面**，否则上一轮的产物会给你假绿
  （这条我实测撞到了，且第一反应是「修复没生效」而不是「门是假的」）。
