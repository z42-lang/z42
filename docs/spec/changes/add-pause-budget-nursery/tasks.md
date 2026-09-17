# Tasks: 按停顿预算自适应 nursery

> 状态：🟡 待 User 确认（阶段 6.5 gate）| 创建：2026-09-17
> 变更类型：`vm`（GC 运行时行为；不改语言 / IR / zbc 格式）
> 前置：`add-incremental-major-gc` M0~M2b 已合（major 停顿已与堆大小脱钩，剩余最大停顿全部来自 minor）

## 进度概览
- [ ] 1: 代价模型（纯函数 + 单测）
- [ ] 2: 接线（minor 结束喂数据 / 闸门读自适应值 / 退避封顶）
- [ ] 3: 旋钮与诊断
- [ ] 4: 验收（停顿 / RSS / 墙钟 / 产物一致）+ 文档

## 1: 代价模型
- [ ] 1.1 `gc/arc_heap/pause_budget.rs`（NEW）：`PauseBudget{ target_us, cost_ns_per_byte(EWMA), nursery }`，
      `observe(pause_us, consumed_bytes) -> next_nursery`；α=1/4、单次限幅 ±50%、夹 `[2M, 64M]`
- [ ] 1.2 `pause_budget_tests.rs`（NEW）：收敛 / 限幅 / 夹紧 / 关闭四组

## 2: 接线
- [ ] 2.1 `arc_heap.rs` + `construct.rs`：持有 `PauseBudget`；显式 `Z42_GC_NURSERY_BYTES` ⇒ 关闭自适应（并打 trace 说明）
- [ ] 2.2 `control.rs`：minor 分支在算完 `pause_us` 后 `observe`
- [ ] 2.3 `auto_collect.rs`：`nursery_bytes()` 读自适应值；**闸门 = `min(minor_gate * backoff, 预算 nursery)`**（design D4）
- [ ] 2.4 `generational.rs`：把本次 minor 消耗的字节（`used_before - baseline`）交给调用方（不新增计数器）
- [ ] 2.5 `auto_collect_tests.rs`：退避 ×16 时闸门不超过预算 nursery 的回归测试

## 3: 旋钮与诊断
- [ ] 3.1 `config.rs` / `config/parse.rs` / `config/knob_table.rs`：`Z42_GC_PAUSE_TARGET_MS`（默认 10，`0` = 关）
- [ ] 3.2 `Z42_GC_PHASES`：nursery 变化时一行（每字节代价 / 新 nursery / 晋升年龄 / 本次晋升字节）
- [ ] 3.3 wasm32：编译期关闭自适应（`now_us` 在 wasm 是计数器不是微秒）

## 4: 验收与文档
- [ ] 4.1 性能验收表（design「Testing Strategy」）：semantics / 大堆两档 ≤ 10 ms；09 墙钟 ≤ +5%；总停顿 ≤ +10%；RSS ≤ +5%
- [ ] 4.2 `xtask test` GREEN + `Z42_GC_SLICE_MS=0.05` 全套 stdlib（两个自适应回路同时工作）
- [ ] 4.3 `docs/internals/src/runtime/gc-tuning.md`：旋钮表 + 「nursery 不再是常量」；
      `gc-incremental-major.md` 的「剩余停顿来自 minor」指向本 change
- [ ] 4.4 归档本 change；memory 更新；若达标则把 bench 门禁的 `--pause-cap-ms` 收紧到 10

## 备注
- 教训 51：改「老年代流入量」的策略必须在 4M / 8M nursery 下同时验证 —— 本 change **改的就是 nursery 本身**，
  所以验收表里必须带 RSS 与晋升字节两列（design D3：缩 nursery 会过早晋升）。
- 不删徒劳退避（`add-incremental-major-gc` 1.10 已证伪：删了 `09_alloc_ctorless` 墙钟回归 70~160%），只给它封顶。
