# 超级指令融合（super-instruction fusion）

> 对齐：2026-08-01（change `interp-superinstr-fusion`）。代码：
> `metadata/superinstr.rs`（框架 + 识别器）、`interp/ops.rs`（`eval_cmp` 共享原语）、
> `interp/mod.rs`（exec 循环消费）、`metadata/bytecode.rs`（`Function.fused_tails` 缓存）。

## 为什么

解释器每条 IR 指令付一次 dispatch（`exec_instr` 大 match → helper）。循环条件是最热的形态：
每轮 `cmp %t, %i, %n`（算 bool 存 `%t`）+ `BrCond %t`（读回 `%t` 再分支）——**两次 dispatch +
一次 bool 存·读往返**。JIT 早已把 `BrCond` 特化（读 bool payload + cranelift `brif`），**解释器没有**。

**超级指令**：把一个 block 的**尾部模式**识别成一条融合步，一次执行掉，省下 dispatch。

## 框架（可扩展——"处理流程框架，便于后续补充"）

```
metadata/superinstr.rs
├── enum SuperInstr        // 融合形态，一个 pattern 一个 variant
│     └── CmpBr { op,a,b,dst,t_blk,f_blk }   // v1：cmp + BrCond
├── enum CmpOp             // 比较算子（与 interp 标量 cmp handler 共享语义）
├── SuperInstr::recognize(block, targets) -> Option<SuperInstr>   // 规则表（加规则=加一条 arm）
└── compute_fused_tails(blocks, branch_targets) -> Vec<Option<SuperInstr>>   // load 期一次性识别
```

- **识别一次，热路径零成本**：`compute_fused_tails` 在 load 期（`loader.rs`，紧挨 `branch_targets`
  之后）算好，存进 `Function.fused_tails`（`#[serde(skip)]` 运行期缓存，**无 zbc/格式影响**）。exec
  循环只读 `fused_tails[block_idx]`（O(1)），不做 per-iteration 识别。
- **加新规则** = ① 加一个 `SuperInstr` variant；② `recognize` 加一条 arm；③ 后端加一个 handler。
  框架、缓存、load 接线都不动。候选：`load+arith`、`arith+store`、`cmp+cmp&&` 等。

## 后端 + 共享

- **解释器**（唯一 v1 消费者）：exec 循环里，若 `fused_tails[block_idx]` 命中 `CmpBr`，执行 block
  的**前 n-1 条**指令，然后跑融合步——经**共享原语 `ops::eval_cmp`**（同一个函数也被独立的
  `Lt`/`Le`/…/`Eq`/`Ne` handler 调用，比较逻辑只有一处）算 bool、分支，**保留回边 safepoint + OSR
  交接**（与 `BrCond` 终结子逐一对齐）。跳过一次指令 dispatch + bool 重读。
- **JIT**：已有等价的 `BrCond` 原生特化（`translate.rs`），路径正交，v1 不动。识别器**刻意放在
  `metadata`（非 `interp`）**，将来 JIT 若要统一走这套识别，直接复用、不必搬代码。

## v1 规则：CmpBr

`cmp %dst, %a, %b`（block 最后一条）+ `BrCond(%dst)` 终结子 → `CmpBr`。融合执行：算比较 → 直接跳
`t_blk`/`f_blk`。`%dst` 仍写回（安全——任何其它读者不受影响；免去活跃性分析）。

> **未来精化**：`%dst` 若单读（只被这条 BrCond 读，循环条件恒如此）可**跳过 bool 写回**再省一点——
> 需一次性 per-function reads 扫描（框架已留 `store_dst` 扩展位）。

## 类型化维度（interp-typed-superinstr，2026-08-01）

融合识别接入编译器 emit 的 `reg_types`（`compute_fused_tails` 收 `&func.reg_types`）：`CmpBr` 的
两操作数若 `reg_types` 确认为**整型**（`is_integer()`：I8..U64——运行期都存成 `Value::I64`）则置
`typed: true`，interp 用 `ops::eval_cmp_i64` 做 **unchecked i64 比较**，跳过 `Value` 判别分支
（`Value::as_i64_unchecked`：index 仍 bounds-checked，只 type 提取 unchecked，靠与 JIT
`is_i64_typed` 同源的 reg_types 不变量）。循环条件 `i<n` 是这条的热路径。

- **为什么用 `is_integer` 而非 `is_i64`**：编译器把 loop counter emit 成 **I32**，用 I64-only 门
  会让 typed 永不触发（实测：先 `0 typed`，改后每循环 `1 typed`）。narrow int 运行期均 `Value::I64`，
  故 unchecked i64 提取对全部整型安全；比较仍是 signed-i64，与既有 `numeric_lt` 逐字节一致。
- **诚实边界**：interp 的 `Value` 是 tagged 枚举，拿不到 JIT「丢 tag、裸 i64」的大头；typed 只省掉
  可预测的判别分支，故是 single-digit% 量级。移动端（iOS/Android/WASM 纯 interp）是主要受益方。
- **实测**（`04_c2_p1_arith_loop`，interp，同二进制 best-of-7，macOS aarch64）：typed **337.7→327.5ms
  ≈ 3%**（最热的紧算术循环，最有利场景）；整个融合框架（#93 untyped + typed）vs 无融合 = 368.0→
  327.5ms ≈ 11%。一般代码循环更少更冷，收益低于此。
- **调试 / A-B**：`Z42_FUSION_DEBUG=1` 打印 `N typed i64` 计数；`Z42_NO_TYPED_FUSION=1` 强制走
  untyped 路径（load 期读一次，与 `Z42_NO_FUSION` 平行），用于同二进制测 typed 净收益。

> **算术链融合（deferred）**：把 `t=a+b; d=t*c`（`t` 单用）这类中间值链融成一步——需把 per-block
> tail 结构扩成 per-instruction 融合表 + 重构 dispatch 热循环 + 单用 reads 分析，ROI（single-digit%）
> 不抵热路径重构风险，暂不做（2026-08-01 裁决）。将来要做时在此展开设计。

## 效果 + 验证

- A/B（同二进制，`Z42_NO_FUSION` 开关，`--mode interp`，best of 7）：**04_arith 504→477ms（~5.4%）**、
  05_poly 2978→2931ms（~1.6%）。紧凑数值循环收益最明显；arith 本身算术受限，故 ~5% 已是这类的合理量级。
- 正确性：`test e2e`（含 jit 模式）输出与融合前**逐字节一致**——融合不改可观察语义。
- 调试：`Z42_FUSION_DEBUG=1` 打印每函数融合的 block 数；`Z42_NO_FUSION=1` 关闭融合走原路（A/B 用）。
