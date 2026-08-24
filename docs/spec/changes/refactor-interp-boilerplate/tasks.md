# Tasks: refactor-interp-boilerplate（M5 解释器内部样板收敛）

> 状态：🟢 已完成 | 创建：2026-08-24 | 完成：2026-08-24
> 类型：refactor（纯重构，行为字节不变）—— 最小化模式

**变更说明：** 收敛 interp 层四处重复样板（OOM 异常构造 ×7、resolved 缓存加载宏 ×10、
「主模块→lazy loader」函数解析 ×3、TypeTag 常数镜像），来源 `docs/runtime_review.md` M5 / #8。
**原因：** 消除逐字复制的样板，单一真相源，降低「新增指令时遗漏一处」的面。
**文档影响：** `docs/runtime_review.md`（勾 #8 状态）；`src/runtime/src/interp/README.md`（功能索引若涉及新入口）。

## 进度概览
- [x] 1. OOM 异常构造 → `make_oom_exception` 辅助
- [x] 2. resolved 缓存加载 → `cached_token!` 宏
- [x] 3. 「主模块→lazy loader」函数调用 → 复核后**刻意不做**（见备注）
- [x] 4. TypeTag 常数 → 复用 `metadata::types::TAG_*` 单一源
- [x] 5. 验证 + 文档同步

## 1. OOM 异常构造收敛
- [x] 1.1 在 `exception/mod.rs` 加 `pub fn make_oom_exception(ctx, module, msg: String) -> Value`
      （封装 set_strict_oom(false) → make_stdlib_exception(Std.OutOfMemoryException) → set_strict_oom(true)）
- [x] 1.2 替换 7 处站点：exec_call.rs ×2、exec_array.rs ×4、exec_object.rs ×1

## 2. resolved 缓存加载宏
- [x] 2.1 在 exec_instr.rs 定义 `cached_token!($site, $field)` 宏（`resolved.filter($site!=UNRESOLVED).and_then(get)`；
      `$site` 传 arm 的 `_site_idx`，`resolved` 自由引用同 `site_idx!`，token 展开一致）
- [x] 2.2 替换 9 处（method/cross/builtin/type/field_ic ×2/vcall_ic/static_field ×2），`.copied()`/`.map()` 尾链由调用方保留

## 3. 函数解析统一入口 —— 复核后刻意不做
- [x] 3.1 复核：review 写于 2026-07-05，其列的 exec_call:94-105/:209-216 等站点在 #4/H3(#273) 后已重构。
      当前 origin/main 仅剩 2 处「resolve→exec」站点且**形状分叉**：CallIndirect（预建 args + none→bail）
      vs exec_object ctor（args 按分支惰性建·prepend obj_val + none→合法隐式 ctor）。统一二者要么改
      args 求值惰性（perf 面）、要么引入 `&Function`/`Arc<Function>` 包装 enum（代价 > 省下的 ~3 行）。
      单站点抽 helper 无去重价值 → 刻意不做（同 #4 M3/M4「刻意不做」判据）。已记 runtime_review.md M5。

## 4. TypeTag 常数单一源
- [x] 4.1 目标已随 #4/H3 从 exec_value.rs 迁至 `semantics.rs`（同一份 T_* 镜像）。改 `semantics.rs`：
      删本地 `const T_BOOL..T_ARRAY`，改 `pub use crate::metadata::types::{TAG_BOOL as T_BOOL, ...}`
      （pub use 免 unused 警告；值逐一核对与 TAG_* 一致）
- [x] 4.2 修正过期注释：semantics.rs 旧「权威在 C# 侧」；jit/translate/emit_fc.rs 的 SEMANTICS 锚注释
      指向真源 `metadata::types::TAG_*`（emit_fc 的 Cranelift 本地 const 仍刻意保留，见 #4）

## 5. 验证
- [x] 5.1 `cargo build --release`（z42vm）无错（3 warning 全 pre-existing：exec_array unused ArrayObj import
      / stack_alloc stats_enabled / resource_registry get_cloned；均非本 diff 触及）
- [x] 5.2 `cargo test --lib` 全绿：z42 960/0（含 semantics_tests 覆盖 convert_value 各 T_* 臂）+ compression 21/0
- [x] 5.3 文档同步：runtime_review.md #8 勾选；无 README 需改（未增删文件、interp 无新对外入口、exception/ 为 4 层无 README）
- [ ] 5.4 PR（本机 z42vm 挂起 → 完整 xtask GREEN 以 CI 为准，见 memory）

## 备注
- 纯搬移/收敛，零格式 bump，行为字节不变（宏展开 token-identical、常量值逐一核对、OOM helper 逻辑逐字封装）。
- 净 −17 行（72 插入 / 89 删除，7 文件）。
- 本机 z42vm 退出期挂起（见 memory），完整 GREEN 交 CI；本地以 cargo test --lib + build 为门禁。
- 任务 3 复核后刻意不做：review 已过期，当前代码仅 2 处且形状分叉，统一得不偿失。
