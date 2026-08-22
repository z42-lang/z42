# Tasks: REPL Tab 缩进网格吸附

> 状态：🟢 已完成（实现+目标验证）| 创建：2026-08-22 | 完成：2026-08-22
> GREEN 说明：本地全量 `xtask test` 受 worktree z42vm/xtask 环境死锁阻挡（UE 卡死进程、
> `test` 家族与本改动无关地挂起），完整 GREEN 以 PR 的 CI 为权威门禁（见 memory「CI 权威」）。

## 进度概览
- [x] 阶段 1: Rust 动作协议（`insert:`）
- [x] 阶段 2: z42 Tab grid-snap-ceil 策略
- [x] 阶段 3: 测试（golden + 单测 + spike）
- [x] 阶段 4: 文档同步（book + roadmap）；完整 GREEN 交 CI

## 阶段 1: Rust 侧
- [x] 1.1 `repl_editing.rs::parse_action`：加 `insert:<text>` → `Cmd::Insert(1,text)`；删 `indent`；保留 `dedent`
- [x] 1.2 `repl_editing.rs` 头部协议注释更新（dedent + insert:）
- [x] 1.3 `repl.rs` 键绑定注释更新（Tab 用 Insert；无新绑定）

## 阶段 2: z42 策略
- [x] 2.1 `ReplEditing.z42`：加 `_spaces(int n)`
- [x] 2.2 `KeyEdit` tab：`next=((col/4)+1)*4; "insert:"+_spaces(next-col)`；退格不变（`dedent`）；头注更新

## 阶段 3: 测试
- [x] 3.1 `repl_editing/driver.z42` + `expected_output.txt`：Tab ceil 场景（col 0/1/2/4/6/有词/多行）
- [x] 3.2 `repl_editing_tests.rs`：`insert:` 单测；`indent`/`replace:` → unknown
- [x] 3.3 手动跑 z42 golden（同包编 driver+ReplEditing → 自建 z42vm run → diff）— 18/18 匹配
- [x] 3.4 PTY spike：Tab col2 → 4 空格、光标末尾 ✓（Replace 光标归位行首亦实测 → 据此定 Deferred）

## 阶段 4: GREEN + 文档
- [x] 4.1 `cargo build --release`（z42vm）无错
- [x] 4.2 `cargo test --lib repl_editing`（4/4）
- [ ] 4.3 `xtask test` 全 stage GREEN — 本地环境死锁阻挡，交 CI 权威门禁（见头部说明）
- [x] 4.4 `docs/roadmap.md`：Deferred 两行理由更正为「光标需 patch edit_insert_text」+ 新 archive 链接
- [x] 4.5 `docs/book/src/toolchain/repl-input-completeness.md`：加「缩进感知键位」节（Tab grid-snap-ceil + redo/光标两坑）
- [x] 4.6 doc-check 清单核对（触发矩阵逐行）

## 备注
- 范围裁决：User 选「只发 Tab-ceil（光标正确），} 回退 + 退格 floor 维持 Deferred」（Option B）。
  理由：Replace(WholeLine) 虽 redo-免疫，但 rustyline edit_insert_text 不推进光标 → 光标归位行首，
  破坏 `}` 后继续输入；z42 非缩进敏感，视觉美化不值功能性倒退，也不值为此 fork rustyline。
