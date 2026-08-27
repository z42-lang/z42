# Tasks: 收缩 primitive 协议 native interop

> 状态：🟡 进行中 | 创建：2026-08-27
> 变更类型：refactor + fix（删 9 个 builtin + 修 long/ulong hash 截断）
> 文档影响：`src/libraries/README.md`（Primitive 协议表 + 汇总）；无 book 机制页变更（纯 native→脚本搬迁，行为等价）

## 进度概览（本 PR = Stage 1：只迁源，保留 builtin）
- [x] 阶段 1: z42 脚本迁移（12 个 primitive/String 文件）
- [~] 阶段 2: Rust 删 builtin —— **移出本 PR → Stage 2 follow-up**（bootstrap-seed 纪律）
- [x] 阶段 3: 账本 + 文档同步（标 🟡 源迁，builtin 留 Stage 2）
- [ ] 阶段 4: 验证 —— stdlib/自举/jit GREEN 交 CI（Stage 1 修订版重跑）

## 阶段 1: z42 脚本迁移
- [x] 1.1 Int32.z42：Equals→`this==other`；GetHashCode→`this`
- [x] 1.2 Int16/SByte/Byte/UInt16/UInt32.z42：同 Int32（窄整型 hash=`(int)this`）
- [x] 1.3 Int64/UInt64.z42：Equals→`this==other`；GetHashCode→折叠 `(int)(this ^ (this>>32))`
- [x] 1.4 Char.z42：Equals→`this==other`；GetHashCode→`(int)this`；ToLower/ToUpper→ASCII 脚本
- [x] 1.5 Double.z42：Equals→`this==other`；GetHashCode→`BitConverter.DoubleToBits` 折叠
- [x] 1.6 Single.z42：Equals→`this==other`；GetHashCode→`BitConverter.SingleToBits`
- [x] 1.7 String.z42：ToString extern→`return this;`（String 静态类已有多个 `this`-脚本方法先例：IsEmpty/Contains/StartsWith）

## 阶段 2: Rust 删 builtin —— ⛔ **移出本 PR，改 Stage 2 follow-up**
> bootstrap-seed 纪律：已发布 nightly 种子 z42c.zpkg 仍引用 `__int32_equals` 等；零格式-bump 路径
> 当前 VM 直接跑旧种子 → 删 builtin 即 `unknown builtin` panic（实测 PR #310 首版 CI 全红
> resolver.rs:392）。故本 PR（Stage 1）**保留全部 9 个 builtin**，删除等 Stage 1 进 nightly 后另开
> Stage 2 PR 做。（首版已删、现已 `git checkout origin/main` 还原 5 个 Rust 文件。）
- [ ] （Stage 2）convert.rs：删 6 个 int/double/char eq·hash builtin fn
- [ ] （Stage 2）char.rs：删 builtin_char_to_lower/builtin_char_to_upper
- [ ] （Stage 2）string.rs：删 builtin_str_to_string
- [ ] （Stage 2）mod.rs：删 9 个派发登记；corelib/tests.rs：删 char casing 直测

## 阶段 3: 账本 + 文档同步
- [x] 3.1 README.md「Primitive 协议」表：9 项标 ✅ 已删（脚本），注明 change 名
- [x] 3.2 README.md：纠正 char casing「Unicode native」→「ToLower/ToUpper=ASCII 脚本；IsWhiteSpace=Unicode native」
- [x] 3.3 README.md 汇总 + Wave 进度：加 Wave 4 行，总数 ~43→~34

## 阶段 4: 验证（Stage 1 修订版）
- [x] 4.1 cargo build --release（z42vm）—— 编译无错（Rust 未改，builtin 保留）
- [x] 4.5 新增测试：z42.core/tests/primitive_protocol_script.z42（int/long hash 折叠 + char ASCII casing + double/single hash 一致性 + string ToString）
- [x] 4.a cargo test --lib —— 全绿（Rust 已还原 origin/main，char casing 单测保留）
- [ ] 4.2 CI compile-toolchain + test-host —— Stage 1 修订版重跑（首版因删 builtin 全红，还原后应过）
- [ ] 4.4 完整 CI GREEN（stdlib dogfood + bootstrap + jit）—— **PR CI 为权威**

## 备注（追加）
- **首版失败根因（已修）**：删 builtin → 零格式-bump 路径当前 VM 直接跑旧种子 z42c（引用 `__int32_equals`）
  → `unknown builtin` panic（resolver.rs:392）。教训：**删 runtime builtin 前必查「已发布种子是否引用它」**
  （z42c 内部 int hash/eq 必引用 `__int32_*`）；引用则须两阶段跨 nightly（本 PR 只迁源，Stage 2 删）。

## 备注
- 零格式 bump（不动 zbc/zpkg 格式）——规避 two-gen bootstrap format-bump 红（见 memory）
- 删 builtin 是已验证安全操作（wave1-bool-script 先例：`__bool_*` 已删）；native 调用期按名解析，
  无 Rust 内部调用方（已 grep 确认）
- 本机验证受限：z42vm 退出期挂起 + seed 可能偏旧 → 完整 GREEN 以 PR CI 为准
