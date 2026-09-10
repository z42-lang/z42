# Tasks: 晋升年龄与大对象门槛旋钮

> 状态：🟢 已完成（2026-09-08）

| 阶段 | 状态 |
|---|---|
| 1. `Z42_GC_PROMOTION_AGE`（构造期读取） | ✅ |
| 2. `Z42_GC_LOH_BYTES`（进程级） | ✅ |
| 3. 测试 + A/B + GREEN + 文档 + 归档 | ✅ |

## 阶段 1: `Z42_GC_PROMOTION_AGE`

- [x] `gc::promotion_age_from_config()`：读取 + `1..=MAX_GEN_AGE` clamp + 越界警告
- [x] `Region<T>` / `VarRegion` / `ArcMagrGC` 各加 `promotion_age: u8` 字段
- [x] `new_for_mode(generational, promotion_age)` / `with_drop_glue_for_mode(.., promotion_age)`
- [x] `generational.rs` 的所有阈值改读 `self.promotion_age`（含写屏障）
- [x] `PROMOTION_THRESHOLD` 保留为默认值；那句假的 `Z42_GC_TENURE` 注释删掉
- [x] 顺带修写屏障的闭包年龄口径（读块自己的年龄，与 `gen_age_of` 一致）

## 阶段 2: `Z42_GC_LOH_BYTES`

- [x] `LOH_BYTES` static + `set_loh_bytes` / `loh_bytes` / `clamp_loh_bytes`
- [x] `class_for` → `class_for_with_limit(payload, loh)`（纯函数，测试不碰全局）
- [x] VM 构造时应用一次

## 阶段 3: 测试 / A/B / GREEN / 文档

- [x] 五个测试（region 晋升年龄、LOH 分类、LOH clamp、两个 config）
- [x] **A/B：+0.023%**（78.504 → 78.522 G 指令，各三次取中位）—— 噪声内
- [x] 端到端验证四种设定确实改变行为，越界确实警告并 clamp
- [x] `./xtask test` 全绿
- [x] 书里「刻意不做」那一节改写成「折中怎么落地的」，规范冲突解除
- [x] 归档

## 交给后续 change 的发现

1. 本 change **不动任何默认值**（age 2 / LOH 64K）—— 默认值属于 change 4
   `arm-gc-by-default`。这个负载上四种设定的差异都在 3% 以内，没有换默认值的依据。
2. 🔴 仍然是下一个杠杆：**chunk 回收是 O(堆) 而不是 O(young)**，
   给 nursery 能买到的停顿压了地板（见 `add-bounded-nursery` 的 design.md）。

## 验收标准

- 两个旋钮都能改变行为，不设时与今天完全一致
- 越界的晋升年龄警告并 clamp，不静默饱和
- 写屏障零新增全局读；`class_for` 的额外代价落在噪声内
