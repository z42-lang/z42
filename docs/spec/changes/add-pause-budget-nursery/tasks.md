# Tasks: 按停顿预算自适应 nursery

> 状态：🟢 实施完成，待合 | 创建：2026-09-17 | User gate 已过：2026-09-17
> 变更类型：`vm`（GC 运行时行为；不改语言 / IR / zbc 格式）
> 前置：`add-incremental-major-gc` M0~M2b 已合（major 停顿已与堆大小脱钩，剩余最大停顿全部来自 minor）

## 进度概览
- [x] 1: 代价模型（纯函数 + 单测）
- [x] 2: 接线（minor 结束喂数据 / 闸门读自适应值 / 退避封顶）
- [x] 3: 旋钮与诊断
- [x] 4: 验收（停顿 / RSS / 墙钟 / 产物一致）+ 文档

## 1: 代价模型
- [x] 1.1 `gc/arc_heap/pause_budget.rs`（NEW）：`PauseBudget{ target_us, cost_ns_per_entry, bytes_per_entry, survivors, last_post_used }`，
      `observe(MinorSample) -> next_nursery`。**实测改了三处初稿设计**：
      ① 计量单位由「每字节」改为**每条目**（年轻代表里还有幸存者，按字节看不见 —— 见 design D1）；
      ② 代价读数由 EWMA 均值改为**衰减最大值**（均值被便宜的启动期 minor 骗去放大 nursery —— design D2）；
      ③ 限幅由对称 ±50% 改为**缩得快涨得慢**（`[cur/2, cur + cur/8]`），下界 `MIN_NURSERY` 取 4M（User 裁决）
- [x] 1.2 `pause_budget_tests.rs`（NEW）：收敛 / 幸存者吃掉余量 / 非对称限幅 / 夹紧 / 关闭 / 空采样不学习，共 6 组

## 2: 接线
- [x] 2.1 `arc_heap.rs` + `construct.rs`：持有 `PauseBudget`；显式 `Z42_GC_NURSERY_BYTES` ⇒ 关闭自适应；
      **新增 `allowance_unit_bytes`**：老年代余量的计量单位固定取*配置的* nursery，不跟自适应值走
      （不拆的代价实测：nursery 16M→4M 让 major 周期 4→29、墙钟 2.41 s→4.40 s）
- [x] 2.2 `control.rs`：minor 分支在算完 `pause_us` 后 `observe`（含 `young_count()` 作为幸存者数）
- [x] 2.3 `auto_collect.rs`：`nursery_bytes()` 读自适应值；闸门经 `backed_off_gate(minor_gate, backoff, cap)`，
      `cap` 由 `Z42_GC_BACKOFF_CAP` 决定 —— **默认关**（design D4，User 裁决 2026-09-17）
- [x] 2.4 `generational.rs`：`young_count()` 汇总三个 region；`Z42_GC_PHASES` 新增
      `minor roots` / `card seed` / `minor bfs` 三行（是它们定位出「nursery 买不到 10 ms」的原因）
- [x] 2.5 `auto_collect_tests.rs`：`backed_off_gate` 的两条回归（×16/×64 开关两态 + 软上限压过的闸门不被封顶放宽）

## 3: 旋钮与诊断
- [x] 3.1 `config.rs` / `config/parse.rs` / `config/knob_table.rs`：`Z42_GC_PAUSE_TARGET_MS`（默认 10，`0` = 关，clamp `[0.5, 1000]`）
      + `Z42_GC_BACKOFF_CAP`（Bool，**默认关**）
- [x] 3.2 `Z42_GC_PHASES`：nursery 变化时一行（want / 每条目代价 / 每条目字节 / 扫描数 / 幸存者 / 目标 / 晋升年龄）
- [ ] 3.3 wasm32：编译期关闭自适应 —— **未做**，`now_us` 在 wasm 上返回计数器，模型读到的代价无意义但不会崩
      （自适应只会把 nursery 夹在 `[4M, 64M]`）。单列跟进项，不阻塞本 change。

## 4: 验收与文档
- [x] 4.1 性能验收表见 design「Testing Strategy」。**门槛已由 User 调整**：初稿的「全线 ≤ 10 ms」实测
      买不到（nursery 够不到的地板 = 卡表 O(老年代) + 增量 major 的 grey 队列），改为
      「最大停顿显著下降且吞吐不回归」。真实负载 `z42c.semantics` 22.8→14.9 ms（−34%）、墙钟 −0.4%、产物逐字节一致
- [x] 4.2 `xtask test` GREEN
- [x] 4.3 `gc-tuning.md`：两个旋钮行 + 新增「按停顿预算自适应 nursery」一节（含「nursery 买不到的那部分」
      与「退避封顶为什么默认关」两张实测表）
- [ ] 4.4 归档本 change；memory 更新。**`--pause-cap-ms` 不收紧到 10** —— 实测达不到，等增量 minor

## 5: 本 change 挖出、但不在本 change 修的两个 M2b 缺陷
- [ ] 5.1 **minor 重复遍历增量 major 的 grey 队列**：`13_gc_large_heap --large` 上每次 minor 把
      **108 142** 个老条目拷进自己的队列重新遍历。老条目的年轻子节点**卡表已经覆盖**（minor BFS 本身
      就靠这条不入队老 children），所以这部分是白做。原型（只 seed 年轻 grey 条目）实测总停顿 −8%，
      但属于改 M2a 的根集语义 ⇒ 单独开 change + 设计评审
- [ ] 5.2 **切片把 minor 闸门推远**：`rearm_auto_collect` 以*当前* `used` 为锚，而清扫切片也走
      `sub_used_bytes` 落到这里 ⇒ 每个切片把下次 minor 推远至多一个闸门。实测 `z42c.semantics`
      `trip minor gate 18.1M grown 30.2M` —— **超调 67%**。原型（改用上次真回收的水位当锚）把
      semantics 打到 11.5 ms，**但 `--large` 从 22.8 崩到 131.6 ms** ⇒ 有别的交互，需单独 change 设计

## 备注
- 教训 51：改「老年代流入量」的策略必须在 4M / 8M nursery 下同时验证 —— 本 change **改的就是 nursery 本身**，
  所以验收表里必须带 RSS 与晋升字节两列（design D3：缩 nursery 会过早晋升）。
- 不删徒劳退避（`add-incremental-major-gc` 1.10 已证伪：删了 `09_alloc_ctorless` 墙钟回归 70~160%），只给它封顶。
