# Spec: GC 线程本地分配（TLAB）

## ADDED Requirements

### Requirement: 分配热路径无全局锁争用

#### Scenario: 单线程分配走 TLAB fast path
- **WHEN** 一个 mutator 线程连续分配对象（`new`）且当前 TLAB chunk 未满
- **THEN** 每次分配只做线程本地 bump（写 chunk 内槽位 + 前移本地指针），**不获取任何 `region_*` / `inner`
  进程全局 Mutex**
- **AND** 分配返回的 `GcRef` / `VarGcRef` 句柄指向稳定地址（chunk 不移动），与旧路径语义一致

#### Scenario: TLAB chunk 填满触发批量领取
- **WHEN** 线程本地 chunk 的槽位/字节用尽，需要再分配
- **THEN** 线程在 `region_*` Mutex 下**一次性**领取一个新 chunk（Region\<T\>=256 槽 / VarRegion=64KB），把旧
  chunk 的填充量批量并回 region 元数据（`initialized` / `young_list` / `all_blocks`），随后继续本地 bump
- **AND** 领取新 chunk 时按 chunk 容量**批量**更新 `used_bytes` / `allocations`（原子），不再逐对象更新

#### Scenario: 多线程并行分配无相互阻塞
- **WHEN** N 个 mutator 线程（并行编译 chunk）各自持有独立 TLAB 同时分配
- **THEN** 各线程在自己的 chunk 内 bump 互不加锁；只有 chunk 领取/retire 短暂持有 region 锁
- **AND** 分配的对象在各自 chunk 内地址不重叠、无数据竞争

### Requirement: TLAB 与 GC（sweep / mark / 分代）一致

#### Scenario: safepoint retire TLAB
- **WHEN** collector 发起 STW（`request_gc_pause`），各 mutator 在 park 前
- **THEN** 每个 mutator retire 自己的活跃 TLAB chunk（发布最终 high-water 填充量，把已填槽位并回 region 元数据）
- **AND** collector 开始 mark/sweep 时，region 元数据完整覆盖所有已分配对象（无「TLAB 里已填但 region 不知道」
  的漏扫）

#### Scenario: sweep 不读半填充的未初始化槽
- **WHEN** sweep 遍历某个当前被 TLAB 独占、尚未填满的 chunk
- **THEN** 只遍历 `[0, high-water)` 已初始化槽位，**不读** high-water 之后的未初始化 `MaybeUninit` 内存
- **AND** 已回收（tombstone）槽位仍可被后续分配复用（free_list 语义不变）

#### Scenario: TLAB 对象跨线程引用后被正确保活
- **WHEN** 线程 A 在自己 TLAB 分配对象 O，把 O 传给线程 B 后 A 退出
- **THEN** O 所在的 chunk 由 region 持有（不随 VmContext 析构释放），mark 阶段沿 B 的根可达 O → O 被保活
- **AND** 若 O 无人引用，sweep 正常回收 O 所在槽位

### Requirement: 统计与压力检查语义保持

#### Scenario: used_bytes 在 chunk 粒度记账仍触发压力事件
- **WHEN** 累计分配跨过 `gc_near_limit_ratio` / `gc_pressure_ratio` 阈值
- **THEN** 仍 fire `NearHeapLimit` / `AllocationPressure` / `OutOfMemory` 事件（触发时机由逐对象放宽到 chunk
  领取粒度，允许小幅延迟，不允许漏发）
- **AND** auto-collect 仍在压力下触发（经 `external_needs_collect` flag → 下个 safepoint 收集）

#### Scenario: strict_oom 模式退化为精确逐对象路径
- **WHEN** `set_strict_oom(true)` 已启用
- **THEN** 分配走旧的逐对象锁路径（不用 TLAB），保证越界时精确 refund + 返 `Value::Null`
- **AND** 该模式不追求并行加速（strict 是诊断用途，非热路径）

## MODIFIED Requirements

### Requirement: 并行编译默认并发度

**Before:** `ParallelConfig` 默认 `_jobs = 1`（串行），`--jobs N` opt-in；并行任何线程数都比串行慢。

**After:** TLAB 落地并实测 jobs-scaling 转正后，`ParallelConfig` 默认 `_jobs = CpuCount()`（或 auto）；
`--jobs 1` 仍可显式退回串行。验收标准：workspace build 墙钟 jobs=CpuCount 明显 **快于** jobs=1（转正）。

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更（纯 runtime 分配器内部重构 + 编译器侧默认配置翻转）。

## Pipeline Steps

受影响阶段：
- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM runtime（GC 分配器 + safepoint + VmContext）
- [x] 编译器配置（`ParallelConfig` 默认值）
