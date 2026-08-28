# Tasks: 拆分 z42.repl 出 z42.scripting

> 状态：🟡 进行中 | 创建：2026-08-27 | 类型：refactor
> 归属：stdlib-interop-and-repl-split-program 轴 2 PR1

**变更说明：** 把终端交互层（Repl + ReplEditing）从 z42.scripting 拆成独立包 z42.repl（`src/toolchain/repl/`）。
**原因：** 切干净 tier 边界（eval-core vs tty 交互），为后续编译器件收敛进 stdlib 铺路。
**文档影响：** scripting README / 新建 repl README / 更新 toolchain README / design/toolchain/repl.md。

## 进度概览
- [x] 阶段 1：新建 z42.repl 包（Repl + ReplEditing）
- [x] 阶段 2：消环 —— MemberNames 下移 Engine + Completer 改引用
- [x] 阶段 3：接线（interactive / build / runtime 注释）
- [x] 阶段 4：文档同步
- [x] 阶段 5：验证（本地静态自检✓；CI GREEN 待 push）（本地不可验 → CI GREEN）

## 阶段 1：新建 z42.repl 包
- [x] 1.1 `src/toolchain/repl/z42.repl.z42.toml`（name=z42.repl, kind=lib, deps: z42.core + z42.scripting）
- [x] 1.2 `src/toolchain/repl/src/Repl.z42`：从 scripting 迁入，`Std.Repl.Repl` 删 `MemberNames`，保留 ReadLine/SetCompleter/SetKeyEditor
- [x] 1.3 `src/toolchain/repl/src/ReplEditing.z42`：从 scripting 迁入，命名空间 `Std.Scripting`→`Std.Repl`，注释里 `Std.Scripting.replKeyEdit` 引用刷新
- [x] 1.4 删 `src/toolchain/scripting/src/Repl.z42` + `ReplEditing.z42`
- [x] 1.5 迁测试 `tests/repl_editing/`（driver.z42 `using Std.Scripting`→`using Std.Repl`）到 z42.repl/tests，删 scripting 侧

## 阶段 2：消环（root-cause）
- [x] 2.1 `Engine.z42` 增 `public static extern string[] MemberNames(string fqn)` [Native("__repl_member_names")]
- [x] 2.2 `Completer.z42`：去 `using Std.Repl`；`Repl.MemberNames(key)`→`Engine.MemberNames(key)`；注释内 FQN 刷新
- [x] 2.3 grep 复核：scripting src 内无残留 `Std.Repl` / `Repl.` 引用（除注释历史提及）

## 阶段 3：接线
- [x] 3.1 `interactive_main.z42`：`SetKeyEditor("Std.Scripting.replKeyEdit")`→`"Std.Repl.replKeyEdit"`（SetCompleter 保持 Std.Scripting.replComplete）
- [x] 3.2 `z42.interactive.z42.toml`：deps 增 `"z42.repl" = "0.1.0"`
- [x] 3.3 `xtask_toolchain.z42`：`_buildScriptingLib` 建完 z42.scripting 后追加建 z42.repl（Z42_LIBS=combined，output→libs）+ 并入 combined
- [x] 3.4 runtime 注释：`repl.rs` / `repl_editing.rs` 里 `Std.Scripting.ReplEditing`→`Std.Repl.ReplEditing`

## 阶段 4：文档同步
- [x] 4.1 `src/toolchain/repl/README.md`（六段制）
- [x] 4.2 `src/toolchain/scripting/README.md`：功能索引 / 核心文件去 Repl·ReplEditing，加关联到 z42.repl
- [x] 4.3 `src/toolchain/README.md`：子目录表增 `repl/`
- [x] 4.4 `docs/design/toolchain/repl.md`：反映 eval-core（scripting）/ tty 交互（repl）拆分

## 阶段 5：验证
- [x] 5.1 本地静态自检：grep 确认无循环依赖（scripting↛repl）、FQN 引用一致、无残留旧路径
- [ ] 5.2 push → PR → CI GREEN（`xtask build toolchain` 编两包 + `cargo test --lib` repl 单测 + bootstrap）
- [ ] 5.3 CI dist smoke（`z42 repl --config -c "1+2"`→3，验 z42i→scripting→repl 运行期解析）

## 备注
- **本地不可验**：种子墙（建不了 z42c）+ z42vm 退出挂起 → GREEN 判定以 CI 为准（bootstrap-seed.md 阶段 8 约定）。
- **packages.toml 不改**：z42.repl.zpkg 建到 stdlib libs 目录 → stdlib-glob 自动入包。
- MemberNames 下移是消环唯一干净解，native 名 `__repl_member_names` 不变 → runtime 零改动（仅注释）。
