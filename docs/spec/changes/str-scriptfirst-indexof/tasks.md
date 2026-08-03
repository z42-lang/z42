# Tasks: Script-First 字符串搜索（char[] view + 脚本 IndexOf）

> 状态：🟡 实施完成，warm 验证过；full gate 待 PR/CI | 创建：2026-08-03 | 类型：vm + stdlib

## 进度概览
- [x] 阶段 1: VM bulk view 原语 `__str_to_chars` + 注册 + Rust 单测
- [x] 阶段 2: stdlib ToCharArray→bulk；IndexOf→char[] 脚本
- [x] 阶段 3: 正确性 + 性能 warm 验证
- [ ] 阶段 4: 完整 GREEN + 文档 + 归档（PR/CI）

## 阶段 1: VM
- [x] 1.1 `corelib/string.rs`：`builtin_str_to_chars`（bulk scalar char[]，GcRef<ArrayObj>）
- [x] 1.2 `corelib/mod.rs`：注册 `__str_to_chars`（删曾试的 `__str_index_of`）
- [x] 1.3 Rust 单测：scalar（"héllo"→5）+ 空串；`cargo test corelib::string` 绿

## 阶段 2: stdlib
- [x] 2.1 `String.z42`：`ToCharArray()` per-char → `[Native("__str_to_chars")]` bulk
- [x] 2.2 `String.z42`：`IndexOf` → char[] 脚本 over ToCharArray（scalar 语义）
- [x] 2.3 `Contains` 不动（IndexOf>=0 自动获益）；清实验方法（CharsBulk/IndexOfArr/IndexOfNat）

## 阶段 3: 验证（warm）
- [x] 3.1 正确性：ASCII(3/-1/0) + UTF-8 scalar("héllo".IndexOf("llo")==2、日本語==3) + Contains
- [x] 3.2 性能：真实 String.IndexOf 2128ms→248ms（**8.6×**，interp）；三方对比 charAt/charArr/native
- [x] 3.3 剖析记录 + 布局微基准（Vec<char> vs Vec<Value> 仅 1.35× → 数组 packed 暂不做）

## 阶段 4: GREEN + 归档（PR/CI）
- [ ] 4.1 `xtask test` 完整 gate（e2e string goldens / stdlib [Test] / compiler 自举 / cross-zpkg）
- [ ] 4.2 `cargo test` 全绿
- [ ] 4.3 自举不动点 gen1==gen2
- [ ] 4.4 z42.core/tests 加 IndexOf/Contains 回归（ASCII+UTF-8）+ MODE-COMPARISON.md 记录
- [ ] 4.5 perf-vm-iteration Phase 5 勾选 + commit

## 备注
- 大头是解释器**派发**（57×）非布局；char[] 脚本吃掉 per-char CharAt builtin 派发 = 8.6×。
- native per-op（__str_index_of，100×）已否决（违 Script-First）；留升级阶梯最后手段。
- 数组 packed 布局：native scan 仅 1.35× vs 123 处耦合 → 暂不做。
- follow-up：StartsWith/EndsWith/Replace/Split 同款转 char[]；byte-index 快路 API。
