# tasks：fix-generic-ctor-args

状态：🟢 已完成（2026-09-25）

## 代码
- 🟢 `MemberResolver._substGenericSig`：`private` → `internal`。
- 🟢 `ConstructTyper._chkCtorSubstArgs`（新）+ `_bindCtorArgs` 加 `inst` 形参。
- 🟢 🔴 **两个返回点都调**（`_adaptArgs` 分支提前 return —— 初稿只补尾部，行为一字未变）。
- 🟢 `ExprTyper` 的 base-ctor 调用点传 `null`（无实例化实参可代换，行为恒等）。

## 验证
- 🟢 `new Box<int>("x")` → E0402；`n.Set("x")` → 同一句 E0402（两条路同口径）。
- 🟢 **阴性对照**：退回修复重建 → examples 门禁在 `generics/gaps/run.console` 判红；恢复 → 绿。
- 🟢 `xtask test all` / `examples` / `docs` / `lines` / `diagcodes` / `walkers` 全部 exit 0。

## 文档 / 示例（三处联动）
- 🟢 `examples/types/generics/gaps/ctorgap.z42`：注释「编译期零诊断」→「编译期就拦下」。
- 🟢 `run.console`：transcript 从运行期异常改成编译错误。⚠️ **列号以实跑为准**（猜 30、实际 31）。
- 🟢 `docs/learn/src/types/generics.md`：标题「不检查」→「也是检查的」+ 📜 说明旧行为与根因；
  顺带把「两个会崩的组合」改成「一个」（`new T()` 基元那条已随 #803 修好）。

## ⭐ 记下来的两条
- ⭐⭐ **改一个函数里的检查前，先数它有几个返回点。** 初稿只在尾部补，构建通过、行为一字未变
  —— 局部 ctor 全走 `_adaptArgs` 的提前 return。同 ⑧ 那次「nsMap 有四个构建点」。
- ⭐ **「零诊断」类缺陷常常是「机制已有、这条路没接」**：本条的 `CheckSubstitutedArgs` /
  `_substGenericSig` 全是现成的，只是 ctor 没调。先找同族路径怎么做的，比自己设计快得多。
