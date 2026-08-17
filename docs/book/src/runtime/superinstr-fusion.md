# 超级指令融合（super-instruction fusion）

> 对齐：2026-08-17（change `interp-superinstr-fusion` + `interp-frame-presize`）。代码：
> `metadata/superinstr.rs`（框架 + 识别器）、`interp/ops.rs`（`eval_cmp` 共享原语）、
> `interp/mod.rs`（exec 循环消费 + `Frame` 预分配）、`metadata/bytecode.rs`
> （`Function.fused_tails` 缓存 + `Function::reg_file_len` / `Instruction::written_reg`）、
> `metadata/loader.rs`（`build_block_indices` 回填 `func.max_reg`）。

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

## 相关机制：寄存器文件预分配（interp-frame-presize，2026-08-17）

与融合同属 `loader::build_block_indices` 的 **post-load 一次性预计算**族。解决的问题：解释器每次
调用构造 `Frame` 时应按「函数寄存器总数」一次性分配寄存器文件，但 zbc reader 把 `func.max_reg`
恒置 0（该计数不落 wire），loader 又从不回填 → `Frame::new*` 只按**实参数**起步，后续对更高寄存器
的写入逐个命中 `#[cold] Frame::set_grow`（每次 `resize(idx+1, Null)` = 一次 realloc + memmove +
清零）。call-heavy 前端里这是 frame 开销中最大的可攻击块。

### 机制

```
Function::reg_file_len()  ← 权威计数（单一真相），纯函数、不读 self.max_reg
   = 1 + max(
       param_count - 1,                       // 参数占低位寄存器
       max(exception_table[*].catch_reg),     // catch 寄存器：运行时在 catch-install 写，
                                              //   IR DCE 可能删掉最后引用它的指令 → 必须显式折叠
       max(instr.written_reg() for 所有指令))  // 每条指令的 dst
```

- `build_block_indices` 在 block_index / branch_targets / fused_tails 之后追加
  `func.max_reg = func.reg_file_len()`，**所有构建配置**（含 interp-only / wasm，无 jit feature）
  均生效——计数逻辑在始终编译的 `bytecode.rs`，不依赖 JIT。
- `Frame::new*` 从此恒走 `max_reg > 0` 预分配分支，一次 `resize` 到位，热路径不再触发 `set_grow`。
- JIT 复用同一权威：`translate::max_reg`（要最大**索引**）= `reg_file_len() - 1`（`reg_file_len` 恒
  ≥ 1，不会下溢）。`Instruction::written_reg`（"这条指令定义哪个寄存器"）也从 `translate.rs` 上提到
  `bytecode.rs`，interp 与 JIT 共用一份。

### interp/JIT 一致性（一处边界行为对齐）

预分配后，读一个「在范围内但从未写过」的寄存器，解释器从 `bail!("undefined register")` 变为返回
`Null`——**与 JIT 一致**（JIT 早已预分配、读到 Null）。z42c codegen 保证 define-before-use，合法
字节码不会到达此边界；本变更只是消除 interp 比 JIT 更严的历史分歧，非行为回归。

### 效果 + 验证

- A/B（同配方，前端 typecheck 21877 行拼接源，`--mode interp`，best of 6）：baseline 7.45s →
  预分配 7.22s ≈ **1.03× / ~3.1%**；profile 确认 `set_grow` 从热点榜消失、`extend_with` 腰斩。
- 正确性：dump-bound 输出**逐行 identical** + z42c 自举 gen1==gen2 逐字节（纯运行时优化，不改任何
  emit，无格式 bump）。`reg_file_len` 三个折叠维度（param / catch reg / 写入 dst）由
  `metadata/bytecode_tests.rs` 单元测试覆盖。
