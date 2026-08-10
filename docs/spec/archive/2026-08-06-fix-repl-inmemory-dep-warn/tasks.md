# Tasks: fix-repl-inmemory-dep-warn

> 状态：🟢 已完成 | 创建：2026-08-05 | 完成：2026-08-06 | 类型：fix（最小化模式）

**变更说明：** REPL 跨轮引用早前声明的类型（R2 用 `Repl.R1.A`）时，lazy loader 按依赖
`repl_r1.zpkg` 去磁盘查找、找不到而刷 `WARN: cannot read dep zpkg meta`，尽管该包已在进程内驻留、引用其实正常解析。

**原因：** 内存内加载的 REPL 包只以内部 key `__loaded_path__<模块名>`（如 `__loaded_path__Repl.R1`）
注册进 `loaded_zpkgs`，而依赖方按**规范文件名** `repl_r1.zpkg` 记录依赖。两套命名不匹配 →
依赖解析循环认不出「已驻留」→ 走磁盘查找（REPL 包从不落盘）→ 假 WARN。

**根因修复：** 加载任一包时，同时把它的规范 zpkg 文件名 `<包名>.zpkg` 记进 `loaded_zpkgs`。
后续依赖方的解析循环即可在 `loaded_zpkgs` 命中并短路，不再磁盘查找、不再 WARN。
（症状级替代方案「加载失败时静默不 WARN」被否决——那只是掩盖命名不匹配，违反根因修复原则。）

**文档影响：** `docs/design/toolchain/repl.md` carry-forward 段补「内存包的驻留识别」子条（VM 内部机制）。

## 改动文件
- [x] 1.1 `src/runtime/src/metadata/loader.rs` — `LoadedArtifact` 加 `package_name: Option<String>`；
      `assemble_zpkg_artifact` 加参并回填；3 处 zpkg 加载点传 `Some(meta.name)`；zbc 内联构造传 `None`
- [x] 1.2 `src/runtime/src/metadata/lazy_loader.rs` — `register_loaded_artifact` 在插入 `mod_key` 后，
      若 `package_name` 有值则 `loaded_zpkgs.insert("<pkg>.zpkg")`
- [x] 1.3 `src/runtime/src/metadata/lazy_loader_tests.rs` — 回归测试
      `inmemory_package_registers_zpkg_file_as_resident`（R1 载入后 `loaded_zpkgs` 含 `repl_r1.zpkg`；
      R2 依赖 `repl_r1.zpkg` 短路、不建磁盘候选）

## 验证
- [x] 2.1 `cargo build --release`（z42vm）无错（仅 2 个 pre-existing warning，来自 origin/main 的 stack_alloc.rs）
- [x] 2.2 `cargo test --lib`：新回归测试 + 既有 loader(15)/lazy_loader(28) 全绿
- [x] 2.3 端到端 A/B（用新 z42vm 换入种子跑真实 `z42i`）：
      原始种子 VM → 复现 WARN；修复 VM → WARN 消失、`Repl.R1.A{...}` 功能不变
- [x] 2.4 完整 GREEN：独立 worktree（origin/main 干净源）`xtask test` 全绿——
      e2e 217/0 + cross-zpkg 8/0 + multi-exe 1/0 + stdlib 全绿 + compiler 自举 5/5 不动点 + vscode-syntax 同步
- [x] 2.5 文档同步：`docs/design/toolchain/repl.md` carry-forward 段补「内存包的驻留识别」子条

## 备注
- 本修复与并发进行的 loop-alloc（`add-loop-alloc-hoist-reuse`）正交；为物理隔离，在独立 worktree
  `z42-replfix-dep`（基于 origin/main）实施 + GREEN + PR，避免与共享工作树的 loop-alloc WIP 混杂。
- `<包名>.zpkg` 的规范化标识与既有「包名 = 文件 stem」约定一致（C# assembly 模型），
  磁盘加载路径（`load_zpkg_file` 已在插入实际文件名）不受影响。
