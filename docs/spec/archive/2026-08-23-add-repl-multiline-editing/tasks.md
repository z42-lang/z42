# Tasks: REPL 整块多行编辑

> 状态：🟡 进行中（实施 + 本地验证完成，待完整 xtask test + 落地）| 创建：2026-08-23

## 进度概览
- [x] 阶段 1: spike 验证 rustyline 多行关键行为（源码坐实 + PTY 实测）
- [x] 阶段 2: 核心实现（回车重入 + 循环去 initial + Ctrl-C/EOF 分流）
- [x] 阶段 3: 测试与验证（Rust 6/6 + golden 24/24 + PTY 5/5；完整 xtask test 进行中）
- [x] 阶段 4: 文档同步（design D3/D6 修正 + book + roadmap + README）

## 阶段 1: spike（实施前验证假设，勿假设）
- [x] 1.1 `Cmd::AcceptLine` 无条件提交（command.rs:133-158 `(Cmd::AcceptLine, ..)` → Submit，validator 忽略）✅ 源码坐实
- [ ] 1.2 `Cmd::Insert(1, "\n    ")` 在缓冲内插换行 + 缩进后光标正确落其后（Insert 光标语义 #257 已 PTY 验；`\n` case 实施后 PTY 复验）
- [x] 1.3 `ectx.line()` 返回整块含 `\n`、`ectx.pos()` 全局偏移（binding.rs:183/189）✅ 源码坐实；方向键跨行导航 = rustyline 多行默认（实施后 PTY 验）
- [ ] 1.4 bracketed paste：多行粘贴整体入单缓冲、粘贴内换行**不**触发 Enter handler（不叠加缩进）— 实施后 PTY 验
- [x] 1.5 **无需 Validator 兜底**：handler 独控（AcceptLine 无条件提交 + 无 handler 回落默认 ENTER 提交单行）✅ 源码坐实 → Validator 保持空 stub

## 阶段 2: 核心实现
- [ ] 2.1 `parse_action`（repl_editing.rs）扩 `"accept"` → `Cmd::AcceptLine`、`"newline:<ind>"` → `Cmd::Insert(1, "\n"+ind)`
- [ ] 2.2 Enter 键绑定（repl.rs `read_one_line`）：`KeyCode::Enter` → 复用/新增重入 handler；去掉 `initial` 预填路径
- [ ] 2.3 `ReplEditing.z42`：回车策略（`key=="enter"` → accept vs newline+ContinuationIndent，含光标中段判定）
- [ ] 2.4 `Repl.z42`：`ReadLine` 语义升级为整块读取；签名去 `initial`（连带 `__repl_readline` builtin）
- [ ] 2.5 `interactive_main.z42`：主循环塌缩（删 `buf` 累积 / 续读 / initial；元指令在整条语句后判 `.` 前缀）
- [ ] 2.6 `mod.rs`：若新增 builtin 则注册

## 阶段 3: 测试与验证
- [ ] 3.1 `repl_editing_tests.rs`：`parse_action` 新动作单测
- [ ] 3.2 `tests/repl_editing/driver.z42` + `expected_output.txt`：回车策略 golden（accept/newline 各 case，同包编译）
- [ ] 3.3 PTY e2e：续读缩进 / 补全提交 / 粘贴回改 / Ctrl-C 弃缓冲 / 元指令短路（5 场景）
- [ ] 3.4 cargo build (z42vm) 无错
- [ ] 3.5 `xtask test` 全 stage GREEN（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）
- [ ] 3.6 spec scenarios 逐条覆盖确认

## 阶段 4: 文档同步 + 归档
- [ ] 4.1 `docs/book/` REPL 输入完整性页：加「整块多行编辑」机制 + 回车判定数据流 + Deferred 地基
- [ ] 4.2 `docs/roadmap.md` Deferred 表：本 change 解锁粘贴回改/跨行导航；更新 `}`/floor 依赖
- [ ] 4.3 相关目录 README（scripting / interactive）六段核对（功能索引 / 如何测试）
- [ ] 4.4 归档 `docs/spec/changes/add-repl-multiline-editing/` → `docs/spec/archive/YYYY-MM-DD-...`

## 备注
- 环境：worktree `../z42-multiline`（基 origin/main 67664c86）。实施期需供种（.z42/xtask/xtask.zpkg）——
  从 warm 树拷 + 种子 z42c 现建（见 [[fresh-worktree-seed-setup]]）；本机 UE 死锁不挡直接起 z42vm 的 golden/PTY 验证，但完整 `xtask test` 需环境恢复或交 CI。
- OQ1（回车机制选型）已在 design Decision 1 定为选项 B（自定义 handler），阶段 1 spike 确认。
- OQ2/OQ3/OQ4 已在 design Decision 2/3/4 给出方案，spike/实施期坐实。
