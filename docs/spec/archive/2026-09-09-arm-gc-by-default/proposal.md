# Proposal: 默认武装 GC —— 把触发条件全部改成相对量（照 Mono SGen）

## Why

三堆路线的 change 4，也是最后一个。今天 `Z42_GC_MAX_BYTES` 不设 = **从不自动回收**，
于是任何长跑的 z42 程序（REPL / 服务）内存无上限增长。要默认武装，原本的框架是
「给 `MAX_BYTES` 挑一个默认值」——**这个框架本身是错的**。

User 提示参考 Mono。核实 Mono SGen 源码（`mono/sgen/sgen-conf.h` +
`sgen-memory-governor.c`，main 分支）：

| 常量 | 值 |
|---|---|
| `SGEN_DEFAULT_NURSERY_SIZE` | `1 << 22` = **4 MB** |
| **`soft_heap_limit` 默认** | `(mword)0 - 1` = **无上限** |
| `SGEN_DEFAULT_ALLOWANCE_HEAP_SIZE_RATIO` | **0.33** |
| `SGEN_DEFAULT_ALLOWANCE_NURSERY_SIZE_RATIO` | **4.0** |

```c
MIN_MINOR_COLLECTION_ALLOWANCE = MIN(nursery × 4.0, soft_heap_limit × 0.33)
allowance = MAX(new_heap_size × 0.33, MIN_MINOR_COLLECTION_ALLOWANCE)
major_collection_trigger_size = new_heap_size + allowance
```

三点直接推翻原框架：

1. **Mono 根本不给堆定默认上限**。「默认武装」不是靠挑一个字节预算实现的。
2. **major 触发是相对增长**（上次 major 后的堆 × 0.33）—— 自适应，10 MB 的脚本和
   4 GB 的服务共用同一套参数，**不需要「按机器内存比例」这种启发式**。
3. **nursery 是绝对值**，和有没有预算无关。

而 `add-bounded-nursery`（#541）实现的两个闸门 —— nursery 默认 `MAX_BYTES/4`、
major 阈值 `MAX_BYTES` —— **都依赖一个必须先存在的预算**，这正是「默认预算定多少」
那道题的来源。

## What Changes

- **所有阈值改成相对量**（Mono 形状）：
  - minor（分代）：自上次回收以来分配满一个 nursery
  - major：`promoted_since_major ≥ MAX(上次 major 后的活集 × 0.33, nursery × 4)`；
    STW 只有一代，同一条规则直接读 `used`
  - `Z42_GC_MAX_BYTES` 降级成**软上限**：不设 = 无上限（同 Mono），设了只压 allowance
    并额外提供一个 near-limit 触发
- **`Z42_GC_NURSERY_BYTES` 默认改成绝对值 32M**，并成为整套策略的计量单位
- **GC 默认武装**：`maybe_auto_collect` 不再因为没预算就直接返回
- **`next_collect_at` 原子**：Mono 的 `major_collection_trigger_size`。
  没有它，默认武装意味着**每次分配都要拿 `inner` 互斥锁**；有了它，分配路径上只剩
  一次 relaxed load + 比较，慢路径每个闸门至多进一次
- 徒劳退避的判据改成「上次回收不到半个闸门」（budget-free）

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/arc_heap/auto_collect.rs` | MODIFY | `decide_trip` / `collection_allowance` / `arm_next_collect` / `rearm_auto_collect`；四个 Mono 常量 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `next_collect_at` / `nursery_bytes` 两个原子 + 测试用 setter |
| `src/runtime/src/gc/arc_heap/construct.rs` | MODIFY | 初始化（nursery 从 config 读一次） |
| `src/runtime/src/gc/arc_heap/alloc.rs` | MODIFY | 分配路径的闸门换成 `next_collect_at`；`sub_used_bytes` 收尾重新装填 |
| `src/runtime/src/gc/arc_heap/interface.rs` | MODIFY | `set_max_heap_bytes` / `set_mode` 后重新装填 |
| `src/runtime/src/config.rs` / `config/knob_table.rs` | MODIFY | 两个旋钮的语义改写（`MAX_BYTES` 不再是武装开关） |
| `src/runtime/src/gc/arc_heap/auto_collect_tests.rs` | MODIFY | 四个测试的前提被本 change 反转，重写 |
| `src/runtime/src/gc/arc_heap_tests/collection.rs` | MODIFY | 增长闸门测试改用显式 nursery |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 相对策略 + 为什么默认武装是可负担的 |
| `docs/spec/changes/arm-gc-by-default/` | NEW | 本变更容器 |

## Out of Scope

- 🔴 **把 `Z42_GC_MODE` 默认切到 `generational`** —— User 已裁决「切，但先补 CI 覆盖」。
  **实测表明还应再往后排一步**：见 design.md「为什么这次不翻模式默认」。
- **把 chunk 回收做成增量的** —— 它是 minor 停顿的大头，也是 nursery 默认值只能停在
  32M（而不是 Mono 的 4M）的直接原因。**这是下一个 change**。

## Open Questions

- [x] 默认预算定多少 → **这道题被 Mono 的形状消解掉了**：不需要默认预算
- [x] 默认 nursery → **32M**，按实测曲线选（见 design.md）
