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

## 效果 + 验证

- A/B（同二进制，`Z42_NO_FUSION` 开关，`--mode interp`，best of 7）：**04_arith 504→477ms（~5.4%）**、
  05_poly 2978→2931ms（~1.6%）。紧凑数值循环收益最明显；arith 本身算术受限，故 ~5% 已是这类的合理量级。
- 正确性：`test e2e`（含 jit 模式）输出与融合前**逐字节一致**——融合不改可观察语义。
- 调试：`Z42_FUSION_DEBUG=1` 打印每函数融合的 block 数；`Z42_NO_FUSION=1` 关闭融合走原路（A/B 用）。
