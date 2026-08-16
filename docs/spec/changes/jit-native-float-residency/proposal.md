# Proposal: JIT 浮点热路径原生化收官 —— float→int 转换 + F64 residency

> 状态：IMPL（vm 类，纯 runtime JIT codegen，无格式 bump、自举字节不动）。
> 承接 `jit-native-float`（native F64 算术）+ `jit-native-convert-float`（int→f64）+ `jit-unbox-regalloc`
> Phase 2C（整数 loop-carried 驻留）——把浮点热环推到与整数热环同等：既 native、又跨迭代驻留。
> 两个逻辑单元、两个可各自 revert 的 commit，一个 PR。

## Why

`jit-native-float` + `jit-native-convert-float` 后，浮点四则/比较/取负 + `(double)i` 已 native，但两处
仍是每迭代开销：

1. **float→int 转换**（`(int)f` / `(long)f` / …）仍每次调 `hr_convert` helper——凡「浮点计算 → 存回
   整型/索引」的热环每迭代必经。
2. **F64 loop-carried 累加器**（`double sum += …`）每迭代 load/store `frame.regs`——整数早在 2C 摆脱了
   这条内存往返（打破 `s+=…` 天花板），浮点还没有。

## What Changes

### Commit 1 — float→int 原生转换（`emit_f64_to_int`）

- 触发：`src` 是 `F64` 且 `to_tag ∈ {I8..U64}`（0x02..=0x09）。
- 实现：Cranelift `fcvt_to_sint_sat` / `fcvt_to_uint_sat`——**饱和 + NaN→0**，逐位复刻 Rust `as`
  （interp `convert_from_f64`）。窄目标饱和到宽度后 sext（有符号）/ zext（无符号）回 i64。
- **`T_U64` 刻意用 `fcvt_to_sint_sat(i64)`**（有符号饱和到 i64），沿用 interp 的 `f as i64`（**非**无符号）。
- dst（整型）经 `store_int` → 可驻留 2B/2C；src（F64）从内存读 → 2C 白名单 disqualify 该 src。

### Commit 2 — F64 residency（2C-for-floats）

- `compute_promotable_regs` 基集纳入 `IrType::F64`；routed 位置 = ConstF64 / F64 Add·Sub·Mul·Div /
  F64 Neg / F64 比较 a·b / Ret。任何 memory-backed op 碰过的 F64 reg 一律 disqualify（同整数不变式）。
- 新增 `load_f64` / `store_f64` 驻留访问汇点（**F64 无块内缓存**——2B `RegCache` 只存整数）。
- prologue 声明 `types::F64` Variable + f64 payload 播种；`Ret` 按 `reg_types` 选 `TAG_F64`/`TAG_I64` spill。
- `emit_f64_binop`/`_cmp`/`_neg` + `ConstF64` 分派改走 `load_f64`/`store_f64` / `def_var`。
- safepoint 不 spill F64（GC 非移动、F64 槽是标量非 root）；OSR 变体照常驻留（Cranelift 自动补 phi arg）。

## Out of Scope

- **F32**：widened 存 `Value::F64`、真 f32 round 未实现 → 留 helper（`is_f64_typed` 排除 F32）。
- **float→char / char↔数值 / f64→f64 恒等 / 盒装 unbox convert**：仍走 `hr_convert`。
- **int→f64 dst 驻留**：`emit_int_to_f64` dst 仍直写内存（disqualify），本轮不驻留（收益边际）。

## 验证

- **正确性**（interp==jit==jitOSR 逐字节）：
  - `ftintcheck.z42`：8 目标宽度 × 正常/边界值（NaN、±inf、越界饱和）→ float→int 全覆盖。
  - `fbench.z42` / `ftest.z42`：F64 累加器 + 全 6 比较 + 四则 + fneg + NaN/±inf/±0 + 混合 int/float。
  - `Z42_OSR_THRESHOLD=1` 强制全环走 OSR 亦逐字节。
- **收益**（隔离，基 origin/main）：
  - float→int 密集环 JIT **2.70×→5.45× interp**（jit 791→392ms）。
  - F64 累加环 JIT **3.23×→6.28× interp**（jit 303→155ms≈2×）。
- **GREEN**：`cargo --lib` 946/0 + `tests`/`bench` `--no-run` + `xtask test all`（e2e interp+vm-jit-
  consistency + stdlib + 自举 5/5 gen1==gen2 逐字节）。纯 runtime，无格式 bump。
