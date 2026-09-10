# Proposal: `Z42_GC_PROMOTION_AGE` + `Z42_GC_LOH_BYTES` —— 三堆旋钮总表的最后两格

## Why

三堆设计的旋钮总表里三个新增旋钮，`Z42_GC_NURSERY_BYTES` 已由 `add-bounded-nursery`
落地，剩下这两个：

- **`Z42_GC_PROMOTION_AGE`** —— 熬过几次 minor 才晋升。今天是硬编码 `const u8 = 2`。
  这一格卡在一条**规范冲突**上：`docs/book/src/runtime/gc-tuning-and-safepoint.md` 有一整节
  「**刻意不做：`PROMOTION_THRESHOLD` 不入 config**」，理由是它被**写屏障**每次堆引用写读取，
  且约 20 处测试把它当编译期常量。**User 已裁决：按书里自己给出的折中走 ——
  构造期读取、缓存进堆的字段，而不是 write-barrier 的运行时读。**
- **`Z42_GC_LOH_BYTES`** —— 变长块走 dedicated chunk 的尺寸门槛，今天硬编码 64 KB
  （= `CHUNK_BYTES`）。这一格是 `fix-loh-never-freed`（#534）当时**裁决推后**的：
  门槛做成运行时值意味着 `class_for`（最热的分配路径）要多读一次全局，而那时没有消费者。
  现在 `add-bounded-nursery` 的 profile 实验需要它。

顺带修一处同族缺陷：**写屏障读闭包 `env` 数组的年龄，而 minor 标记读闭包块自己的年龄**
—— #533 给闭包块加了 `gen_age` 并让 `gen_age_of` 读它，这个屏障却没跟上。`env` 必然先于
闭包块分配（因而永不更年轻），所以两者能不一致：一个「看起来老」的闭包写进老 owner 会
跳过脏卡，而块本身还年轻。

## What Changes

- `Z42_GC_PROMOTION_AGE`：建堆时读一次（`gc::promotion_age_from_config`），
  分发给 `ArcMagrGC` / `Region<T>` / `VarRegion` 各存一份 `promotion_age: u8`。
  写屏障读**普通字段**，热路径零成本。范围 `1..=MAX_GEN_AGE`（3），越界警告 + clamp
- `PROMOTION_THRESHOLD` 常量保留，语义从「值」变成「默认值」—— 约 20 处测试一行不改
- `Z42_GC_LOH_BYTES`：进程级 `static AtomicUsize`，VM 构造时 `set_loh_bytes` 一次；
  `class_for` 多一次 relaxed load。上界硬顶 `CHUNK_BYTES`
- `class_for` 拆出 `class_for_with_limit(payload, loh)` —— 纯函数，测试不用碰全局
- 修写屏障的闭包年龄口径
- 更新书里「刻意不做」那一节：折中方案已落地，冲突解除

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/mod.rs` | MODIFY | `promotion_age_from_config()`：读取 + 范围 clamp + 警告 |
| `src/runtime/src/gc/region/entry.rs` | MODIFY | `PROMOTION_THRESHOLD` 文档改成「默认值」；删掉那句假的 `Z42_GC_TENURE` |
| `src/runtime/src/gc/region.rs` / `region/generation.rs` | MODIFY | `promotion_age` 字段 + `new_for_mode(generational, age)` + `promotion_age()` |
| `src/runtime/src/gc/var_region.rs` | MODIFY | `promotion_age` 字段 + `with_drop_glue_for_mode(.., age)`；导出 `MAX_GEN_AGE` / `loh_bytes` / `set_loh_bytes` |
| `src/runtime/src/gc/var_region/chunk.rs` | MODIFY | `LOH_BYTES` static + `set_loh_bytes` / `loh_bytes` / `clamp_loh_bytes`；`class_for` → `class_for_with_limit` |
| `src/runtime/src/gc/var_region/block.rs` | MODIFY | `MAX_GEN_AGE` 提升可见性 |
| `src/runtime/src/gc/arc_heap.rs` / `arc_heap/construct.rs` | MODIFY | `promotion_age` 字段 + 构造期读取一次 |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | 所有阈值改读 `self.promotion_age`；写屏障的闭包年龄口径 |
| `src/runtime/src/vm_context/construct.rs` | MODIFY | VM 构造时应用 `Z42_GC_LOH_BYTES` |
| `src/runtime/src/config.rs` / `config/parse.rs` / `config/knob_table.rs` | MODIFY | 两个新旋钮 |
| `src/runtime/src/gc/region_tests.rs` / `var_region_tests.rs` / `config_tests.rs` | MODIFY | 五个测试 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 「刻意不做」→「折中怎么落地的」；LOH 门槛为什么是进程级 |
| `docs/spec/changes/add-promotion-age-and-loh-knobs/` | NEW | 本变更容器 |

## Out of Scope

- **GC 默认武装 / 默认值怎么定** → change 4 `arm-gc-by-default`
- **把 chunk 回收做成增量的** → `add-bounded-nursery` 留下的下一个杠杆
- **给 `PROMOTION_AGE` / `LOH_BYTES` 定新的默认值** —— 本 change 只把旋钮开出来，
  默认值一律保持今天的行为（2 / 64K）

## Open Questions

- [x] `PROMOTION_THRESHOLD` 的规范冲突 → **User 裁决：构造期读取**（书里自己给的折中）
- [x] `LOH_BYTES` 的热路径代价 → 实测 **+0.023%**（78.504 → 78.522 G 指令，三次取中位），噪声内
