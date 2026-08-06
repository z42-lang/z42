# Tasks: add-repl-type-metacommand

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06 | 类型：feat（REPL 元指令；仅 interactive_main.z42）

## 进度概览
- [x] 1. `.type <expr>` 实现（interactive_main.z42）
- [x] 2. 成员 inline ghost 修复（repl.rs，User 报告）
- [x] 3. 验证 + 文档同步

## 1. `.type`（interactive_main.z42）
- [x] 1.1 派发链加 `else if (t.StartsWith(".type ")) { _showType(s, t); }`（在 `.types` 精确判之后）
- [x] 1.2 `_showType`：`Script.Eval(s, "(" + expr + ").GetType().Name")` → 打印；空参提示用法；失败打印错误
- [x] 1.3 `_help` 补 `.type <expr>` 一行；语义 = 运行期类型（User 裁决）

## 2. 成员 ghost（repl.rs `identifier_hint`）
- [x] 2.1 去掉「成员上下文 return None」的跳过 → `Console.W` 也 ghost `riteLine`；只保留空词跳过

## 3. 验证
- [x] 3.1 `.type` 管道实测：`"hi"`→String、`1+2`→Int32、数组→Array、undefined→优雅报错 ✓
- [x] 3.2 `cargo build --release`——成员 ghost 改动编译通过
- [x] 3.3 **`xtask build toolchain`**——验证 interactive_main.z42 编译（GREEN 不覆盖 z42.interactive）
- [x] 3.4 完整 GREEN（无 Z42_HOME）全绿：e2e 224/0 + cross-zpkg 8/0 + multi-exe 1/0 + stdlib + compiler 自举 5/5 不动点
- [x] 3.5 文档同步：`docs/design/toolchain/repl.md`（`.type` 已接·运行期语义 + 补全段成员 ghost）
- [ ] 3.6 交互手感验收（User）：`.type` 各例；`Console.W`+打字出灰字 `riteLine`

## 备注
- 仅改 z42.interactive（无 Rust/VM 改动）→ 关键验证是 build toolchain（编译）+ 管道实测；full GREEN 是形式门禁。
- `.type` 可**管道实测**（非交互路径也走元指令派发 + Script.Eval）——不像 Tab/ghost 需 TTY。
- 独立 worktree `z42-repl-dottype`（基于 origin/main 8ad813e5，含 #133）。
- 命名空间补全 / 错误行号 / `.members` 留后续迭代（见 proposal Out of Scope）。
