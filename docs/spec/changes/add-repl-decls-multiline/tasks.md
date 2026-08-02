# Tasks: REPL 多行输入 + 顶层声明累积

> 状态：🔴 待 User 6.5 确认 | 创建：2026-07-27 | 占用子系统：`toolchain`

## 进度概览
- [ ] 阶段 1: 勘定（声明名改写？测试落点？）
- [ ] 阶段 2: 声明累积核心（Script.z42 + ScriptState.z42）
- [ ] 阶段 3: 多行输入接线（interactive_main.z42）
- [ ] 阶段 4: 测试
- [ ] 阶段 5: 验证与文档同步

## 阶段 1: 勘定（写代码前的两处待定，回填 Scope）
- [ ] 1.1 实测：`using Repl.R{N}` 后，自由函数/类的裸引用能否经 `GetStaticScoped` 解析（决定声明名是否需 Rewriter 改写，design D3）
- [ ] 1.2 勘定测试落点：scripting `tests/<name>/` [Test] vs `src/tests/<cat>/` e2e 夹具；回填 proposal Scope 的两个 NEW 测试路径

## 阶段 2: 声明累积核心
- [ ] 2.1 `ScriptState.z42`：新增 `DeclNamespaces: List<string>` + `DeclNames: List<string>`，构造器初始化
- [ ] 2.2 `Script.z42` `_classify`：扩顶层声明识别（类型关键字 / `<T> <name> (` 函数形状），返回符号名 + 类别
- [ ] 2.3 `Script.z42` `Eval`：声明轮分支——重名检测（`DeclNames`）→ prelude 组装（含 `DeclNamespaces` 的 `using` + 声明原文）→ 编译 → `ExtendWithPackage` → `LoadBytes` → `Invoke` → 推进 `DeclNamespaces`/`DeclNames`
- [ ] 2.4 `Script.z42` prelude：对**所有轮**追加 `DeclNamespaces` 的 `using`（使表达式轮也能引用已定义符号）
- [ ] 2.5 确认 perf 不回归：非声明轮不并入 scan、不前进 `VarsRound`/`DeclNamespaces`（保持 O(1)）

## 阶段 3: 多行输入接线
- [ ] 3.1 `interactive_main.z42`：read 循环 `Repl.ReadLine(">>> ")` → `Repl.ReadBlock(">>> ", "... ")`；`null`（EOF）分支不变
- [ ] 3.2 `.help` 文案：补「多行块自动续读」「可定义 fn/class」说明

## 阶段 4: 测试
- [ ] 4.1 声明函数→调用、声明类→实例化、声明+变量共存（spec 三 scenario）
- [ ] 4.2 重名报错 + 会话不破坏、声明编译失败会话不推进（spec 两 scenario）
- [ ] 4.3 多行块端到端（`fn ... {` 续读 → 求值）
- [ ] 4.4 回归：现有变量 carry-forward / using 累积用例仍绿

## 阶段 5: 验证与文档同步
- [ ] 5.1 `cargo build --release`（z42vm）—— 本 change 不改 Rust，确认无连带破坏
- [ ] 5.2 `xtask test`（完整 GREEN gate：e2e + cross-zpkg + stdlib + compiler + vscode-syntax）
- [ ] 5.3 spec scenarios 逐条覆盖确认（验证报告表）
- [ ] 5.4 文档同步：`docs/design/toolchain/repl.md`（状态模型/输入分类/follow-up；`_` 移 Deferred）+ scripting/interactive 两 README 功能索引 + roadmap Deferred Index 加 `repl-future-underscore-var`（+ 若确认 supersede 也 defer 则加 `repl-future-redefine`）

## 备注
- `_`（上次结果）与 fn/class supersede 明确 defer（User 2026-07-27）——记入 repl.md Deferred + roadmap 索引。
- 纯 toolchain 锁；不改 `src/runtime/` / `src/compiler/` 源。
