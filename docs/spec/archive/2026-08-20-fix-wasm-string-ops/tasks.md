# Tasks: fix-wasm-string-ops（str_meta 缓存跨 VM 假命中）

> 状态：🟢 已完成 | 创建：2026-08-20 | 完成：2026-08-20 | 类型：fix（根因修复）
> （3.4 CI 全覆盖分片为落地后 nightly/dispatch 验证；shard 2 独立既有 OOB 见文末备注，另开 change）

**变更说明：** `corelib::str_meta` 的 per-string 字符元数据缓存（`char_len`/`CharAt`
offset 表）是 `thread_local`，**跨 VmContext / heap 拆毁存活**。goldens 在同一线程按序
跑于隔离 VM；旧 VM 拆毁后 `VarRegion` chunk 内存还给系统 malloc，新 VM 在**同一地址**
重新分配块（wasm32 线性内存确定+密集复用地址），新块 bump 分配 **generation=0**，与旧
VM 遗留缓存条目（同址、同 gen=0）**假命中** → 返回**旧字符串的 `char_len`**（观测到
`"".Length==13`、`ToLower(17 字符串)` 只处理 13）。`is_live`（地址+gen）只能防**同 region
内**槽复用（gen 在 tombstone 时 bump），防不住**跨 region 地址复用+gen 归零**。

**为什么 wasm 专属**：wasm32 线性内存地址确定且密集复用；64-bit desktop 系统 malloc 把
新 heap 的 chunk 分散到不同地址，碰撞概率极低，故 desktop 从未触发。属**真实平台 bug，
非平台能力差异**——必须修，不排除。

**原因（根因）：** 缓存作用域错误——它按 GC 块**地址**索引，却未随「地址空间（heap）」
的生命周期作用域化。

**修复（根因）：** 给每个 heap 一个**全局单调 epoch**（`ArcMagrGC` 构造时从全局
`AtomicU64` 领取，永不复用）。`str_meta` 缓存记住上次 epoch，检测到当前 heap epoch 变化
即**清空缓存**。epoch 单调 ⇒ 即便新 heap 复用旧地址，epoch 必不同 ⇒ 必清 ⇒ 假命中物理上
不可能再发生。同一 heap 内 epoch 恒定 ⇒ 缓存照常命中、零 perf 损失（热路径只多一个
thread_local `u64` 读+比较，无 vtable）。

**文档影响：**
- `str_meta.rs` 模块 Soundness 注释：补「跨 heap epoch 作用域」一节（旧注释只讲同 region gen 守卫，不完整）。
- `docs/book/`：字符串/GC 机制页若有 str_meta 缓存描述则同步（核对）。

## 任务

- [x] 1.1 `gc/heap.rs`：`MagrGC` trait 加 `fn heap_epoch(&self) -> u64 { 0 }` 默认（mock/test heap → 0）
- [x] 1.2 `gc/arc_heap.rs`：全局 `AtomicU64` epoch 分配器 + `ArcMagrGC.epoch` 字段 + `Default` 领取 + impl `heap_epoch`
- [x] 1.3 `gc/ambient.rs`：thread_local `CURRENT_EPOCH` + `HeapGuard` enter/drop 维护 + `current_heap_epoch()`
- [x] 1.4 `corelib/str_meta.rs`：移除诊断插桩；with_meta 检测 epoch 变化即 `cache.clear()`；更新 Soundness 注释
- [x] 2.1 回归测试：`cross_heap_recycled_address_no_false_hit`（heap1 缓存 13-char→拆毁→heap2 同址 "XY"）。
      **验证充分**：临时禁用 clear → 测试确定性失败 `left:13 right:2`（正是原 bug）；启用 → 绿。
- [x] 2.2 还原诊断脚手架：`agent.z42 _oneLine` 复原；删临时 `wasm_toupper_repro.z42`
- [x] 3.1 `cargo build`（desktop）+ `cargo test --lib`：str_meta 7/7、gc:: 287/0 绿
- [x] 文档：`str_meta.rs` 模块 Soundness 补「跨堆 epoch 作用域」；`gc.md` ambient 节补「堆 epoch」段
- [x] 3.2 本地 wasm：`--filter string` Playwright **×3 全绿**（此前稳定 3 失败）；`--shard 1/3`、`--shard 3/3` 全绿
- [x] 3.3 完整 GREEN：`xtask test` → **✅ 全 stage 通过（C#-free），z42c 自举字节不动点 5/5**
- [x] 3.5 `cargo test --lib`（runtime 单测，见 [[xtask-test-excludes-cargo-test]]）
- [ ] 3.4 CI 全覆盖分片验证（wasm nightly/dispatch）——落地后触发

## 备注：shard 2 的独立既有 OOB（不在本 fix 范围）

`--shard 2/3` 本地报 `RuntimeError: memory access out of bounds`（7.5min 后 crash，console 无输出）。
**这是与 str_meta 假命中无关的独立既有 bug**：
- 原始失败 CI（run 32308111031，我修复前）shard 2 即失败——`TimeoutError`（10.4min 超时），是同一
  有问题 case 的不同表现（本地 OOB / CI 挂起）。
- 我的修复内存正交（清缓存只释放内存），**物理上不可能造成越界读**。
- 属 memory「tier2 全覆盖」follow-up 里记的「wasm shard 2 未提取的失败集」。

→ 独立 change 调查（需 bisect 定位哪个 case OOB）。**不 bundle 进本 str_meta fix**（scope 纪律）。
