# Tasks: Math.Sign / Clamp 补齐（纯脚本）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式，additive API）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree 实施+验证。

**变更说明：** review §2.6 指出 `Math` 缺 `Sign`/`Clamp`（"三行脚本"）。补 `Sign(double)`/`SignInt(int)`
/`Clamp(double,double,double)`/`ClampInt(int,int,int)`——int/double 变体分名，沿用 Abs/AbsInt 约定
（z42 arity-only mangling 无法承载同 arity 重载，见 compiler-future-typed-overload-resolution）。

**原因：** review §2.6——常用数学 helper 缺失。

**文档影响：** 新增对外 API（Math.Sign/Clamp 族）。z42.math README 功能索引可补一行（本 change 加
      Sign/Clamp 条）；无 book 机制变更。

- [x] 1.1 `Math.z42`：Sign/SignInt/Clamp/ClampInt 纯脚本
- [x] 1.2 回归测试 `math_basics.z42`：Sign（负/正/零 × int/double）+ Clamp（上/下/范围内 × int/double）
- [x] 1.3 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 新增对外 API：z42.math README 功能索引补 Sign/Clamp（下方随 commit 落）；无 book 变更
