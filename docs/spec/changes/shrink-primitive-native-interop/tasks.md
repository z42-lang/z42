# Tasks: 收缩 primitive 协议 native interop

> 状态：🟡 进行中 | 创建：2026-08-27
> 变更类型：refactor + fix（删 9 个 builtin + 修 long/ulong hash 截断）
> 文档影响：`src/libraries/README.md`（Primitive 协议表 + 汇总）；无 book 机制页变更（纯 native→脚本搬迁，行为等价）

## 进度概览
- [x] 阶段 1: z42 脚本迁移（12 个 primitive/String 文件）
- [x] 阶段 2: Rust 删 builtin（3 impl 文件 + 派发表）
- [x] 阶段 3: 账本 + 文档同步
- [x] 阶段 4: 验证（cargo build + cargo test --lib 全绿；stdlib/selfhost GREEN 交 CI/PR）

## 阶段 1: z42 脚本迁移
- [x] 1.1 Int32.z42：Equals→`this==other`；GetHashCode→`this`
- [x] 1.2 Int16/SByte/Byte/UInt16/UInt32.z42：同 Int32（窄整型 hash=`(int)this`）
- [x] 1.3 Int64/UInt64.z42：Equals→`this==other`；GetHashCode→折叠 `(int)(this ^ (this>>32))`
- [x] 1.4 Char.z42：Equals→`this==other`；GetHashCode→`(int)this`；ToLower/ToUpper→ASCII 脚本
- [x] 1.5 Double.z42：Equals→`this==other`；GetHashCode→`BitConverter.DoubleToBits` 折叠
- [x] 1.6 Single.z42：Equals→`this==other`；GetHashCode→`BitConverter.SingleToBits`
- [x] 1.7 String.z42：ToString extern→`return this;`（String 静态类已有多个 `this`-脚本方法先例：IsEmpty/Contains/StartsWith）

## 阶段 2: Rust 删 builtin
- [x] 2.1 convert.rs：删 builtin_int32_equals/int32_hash_code/double_equals/double_hash_code/char_equals/char_hash_code（6）
- [x] 2.2 char.rs：删 builtin_char_to_lower/builtin_char_to_upper（2）
- [x] 2.3 string.rs：删 builtin_str_to_string（1）
- [x] 2.4 mod.rs：删对应 9 个派发表登记行 + 留迁移注释（对齐 wave1-bool-script 风格）
- [x] 2.5 corelib/tests.rs：删 char_to_lower/upper 的 Rust 单测（行为改由 z42 stdlib 测覆盖）

## 阶段 3: 账本 + 文档同步
- [x] 3.1 README.md「Primitive 协议」表：9 项标 ✅ 已删（脚本），注明 change 名
- [x] 3.2 README.md：纠正 char casing「Unicode native」→「ToLower/ToUpper=ASCII 脚本；IsWhiteSpace=Unicode native」
- [x] 3.3 README.md 汇总 + Wave 进度：加 Wave 4 行，总数 ~43→~34

## 阶段 4: 验证
- [x] 4.1 cargo build --release（z42vm）—— 无新增编译错误 / 警告（3 个 pre-existing 警告与本改无关）
- [x] 4.5 新增测试：z42.core/tests/primitive_protocol_script.z42（int/long hash 折叠 + char ASCII casing + double/single hash 一致性 + string ToString）
- [x] 4.a cargo test --lib —— 1006 + 21 全绿（删 2 个 char casing Rust 单测后）
- [ ] 4.2 xtask test stdlib —— **交 CI**（本机 z42vm 退出期挂起 hazard，见 memory；不本地跑避免僵进程）
- [ ] 4.3 xtask test compiler（自举字节不动点）—— **交 CI**
- [ ] 4.4 完整 xtask test + verify-selfhost + jit —— **PR CI 为权威 GREEN**

## 备注
- 零格式 bump（不动 zbc/zpkg 格式）——规避 two-gen bootstrap format-bump 红（见 memory）
- 删 builtin 是已验证安全操作（wave1-bool-script 先例：`__bool_*` 已删）；native 调用期按名解析，
  无 Rust 内部调用方（已 grep 确认）
- 本机验证受限：z42vm 退出期挂起 + seed 可能偏旧 → 完整 GREEN 以 PR CI 为准
