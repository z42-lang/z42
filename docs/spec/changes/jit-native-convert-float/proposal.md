# Proposal: JIT 原生 int→f64 转换

> 状态：IMPL（vm 类，纯 runtime JIT codegen，无格式 bump、自举字节不动）。
> 与 `jit-native-float`（native F64 算术）互补、正交——各自独立 PR，合起来覆盖浮点热环全链。

## Why

`jit-native-float` 让 `double` 四则/比较/取负走原生，但**混入 int 的浮点环**里的
`(double)i` / `(double)count` 转换仍每次调 `hr_convert` helper（native→Rust call + tag 分派）。
`(double)i` 几乎是每个「int 索引 + double 累加」热环的每迭代必经——一块明显的 per-iteration
helper 开销。

## What Changes

给 `Convert` 加 **int→f64 原生快路径**（`emit_int_to_f64`）：

- 触发：`src` 是整数类型（I8..U64）且 `to_tag ∈ {T_F32=0x0A, T_F64=0x0B}`。
- 实现：读 src 的 i64 payload，`fcvt_from_sint`（signed src）/ `fcvt_from_uint`（unsigned src，
  按 src reg_type 符号性选），存 `TAG_F64`。
- **精度**：interp `x as f64` / `u as f64` 对 F32 和 F64 目标**都用全 f64 精度**（不 round 到 f32），
  故单条 fcvt 覆盖两个 to_tag、与 interp 逐位一致。
- **2C 无交互**：Phase 2C 白名单把「非 int-convert 的 src」disqualify，故 int→float 的 src 永不是
  resident Variable → `emit_int_to_f64` 直接读内存正确。

## Out of Scope

- **float→int**（`f as iN`）：需 saturating `fcvt_to_sint_sat`/`fcvt_to_uint_sat` + NaN→0 + 按目标
  宽度/符号收窄，比 int→float 复杂；且热环里不如 int→float 常见 → 独立 follow-up。
- **F32 精度语义**：本 change 依赖「interp int→F32 也用全 f64 精度」这一事实；若将来 F32 收紧为真 f32，
  需回来 fdemote。

## 验证

- 正确性：`ftest.z42`（`(double)i`/`(double)seed`/`(double)(i*2)` 混合 + 全浮点边界）interp==jit==jitOSR 逐字节。
- 收益（隔离本 change，基于 main、无 native 算术）：float 环 JIT **1.59×→2.09× interp**（`(double)i`
  per-iteration helper 消除）。与 `jit-native-float`（1.59×→2.78×）叠加覆盖浮点热环全链。
- GREEN：`cargo --lib` + `xtask test e2e` 逐字节 interp==jit + 自举 5/5 gen1==gen2 + stdlib。
