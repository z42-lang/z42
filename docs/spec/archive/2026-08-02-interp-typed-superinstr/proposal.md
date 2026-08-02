# Proposal: 解释器类型化超级指令（Lever 2）

## Why

编译器已为每个寄存器 emit `reg_types: Box<[IrType]>`（`TypedReg` 系统），**JIT 用它**做原生 i64
特化，**解释器却完全丢弃**——每条 `Add`/`Lt` 都对 tagged `Value` 做完整 match。移动平台
（iOS/Android/WASM）**只跑 interp**（沙箱/App Store 禁 JIT），故 interp 的每一点系统性提速都直接
惠及移动端。

**诚实上限**：interp 的 `Value` 是 `#[repr(C,u8)]` tagged 枚举，拿不到 JIT 那种"丢 tag、裸 i64"
的大头；reg_types 快路径只能省掉可预测的判别分支。故本 change 目标是 interp 紧算术/循环代码
**~single-digit%**，用**扩展 #93 超级指令框架**的方式做（类型化 + 算术链融合复利，而非叠加），把
收益集中在最热的循环条件与算术链上。

## What Changes

- `Value` 加 `as_i64_unchecked` / `as_bool_unchecked`（`unreachable_unchecked`，靠 reg_types 不变量）。
- #93 的 `SuperInstr::CmpBr` 加类型维度：`reg_types[a],[b]` 均 I64 时识别为**类型化 CmpBr**，interp
  用无分支 unchecked i64 比较（跳过 dispatch + tag 检查）。
- 新增**算术链融合**（`arith-chain`）：单用中间值的连续算术（`t=a+b; d=t*c`，`t` 只被下一条读）
  融合成一条超级指令，省 dispatch + 中间 `Frame::set`；typed 时链内 unchecked i64。
- `compute_fused_tails` 接收 `reg_types` 做类型判定。

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/runtime/src/metadata/types.rs` | MODIFY | `Value::as_i64_unchecked` / `as_bool_unchecked` |
| `src/runtime/src/metadata/superinstr.rs` | MODIFY | CmpBr 加 `typed`；arith-chain 识别；`recognize`/`compute_fused_tails` 收 `reg_types` |
| `src/runtime/src/metadata/loader.rs` | MODIFY | `compute_fused_tails` 传 `&func.reg_types` |
| `src/runtime/src/interp/ops.rs` | MODIFY | `eval_cmp_i64_unchecked` + 算术链 unchecked 求值 |
| `src/runtime/src/interp/mod.rs` | MODIFY | 消费 typed CmpBr + arith-chain 超级指令 |
| `src/runtime/src/metadata/superinstr_tests.rs` | NEW/MODIFY | 识别器单测（typed CmpBr / chain / 非法回退） |
| `docs/book/src/runtime/superinstr-fusion.md` | MODIFY | 类型化 + 算术链两节 |

**只读引用**：`src/compiler/z42c.semantics/src/FunctionEmitter.z42`（理解 reg_types emit）。

## Out of Scope

- 标准（非融合、非链）算术的 typed 化——需把 reg_types 线程进每个 handler，为 marginal 收益做
  invasive 改动，不值。typed 只覆盖融合超级指令（最热的循环条件 + 算术链）。
- Value 表示改造（去 tag / NaN-boxing）——那是独立大改，非本 change。

## Open Questions
- [ ] arith-chain 的链长上限（先 2-3 条起步，避免识别器复杂度膨胀）
