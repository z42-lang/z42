# Proposal: JIT 原生 F64 浮点算术

> 状态：IMPL（vm 类，纯 runtime JIT codegen，无格式 bump、自举字节不动）。
> 前置：整数原生化程序（P5-B 字段 + 2A 窄整数 + 2B/2C 整数驻留，#192/#196/#201 均合）。

## Why

整数原生化程序收官后，JIT 对**整数**热路径已接近 native，但**浮点**仍全走 helper：
`double`/`float` 的 Add/Sub/Mul/Div/比较/取负都路由到 extern `jit_add`/`jit_lt`/…（native→Rust
call + tag 分派 + 内存往返）。实测纯 `double` 累加环 JIT 仅 **1.59× interp**（vs 整数同构 2C 后
1.75×）——浮点是 native-化程序留下的明显缺口。

## What Changes

给 JIT 加 **F64（double）原生算术快路径**，与 2A 的整数快路径同构：

- `emit_f64_binop`（`fadd`/`fsub`/`fmul`/`fdiv`）——谓词 `is_f64_typed`（dst,a,b 全 `F64`）。
- `emit_f64_cmp`（`fcmp` + `FloatCC`）——谓词 `is_f64_cmp`。**NaN 语义**：Eq/Lt/Le/Gt/Ge 用
  **ordered**（NaN→false）、Ne 用 **unordered-or-not-equal**（NaN!=NaN→true），与 interp 的 Rust
  `f64` `==`/`<`/… 逐位一致。
- `emit_f64_neg`（`fneg`，翻符号位）——谓词 `is_f64_typed_unary`。
- 接入 Add/Sub/Mul/**Div**/Neg/Eq..Ge 各臂：整数分支之后、helper 之前插 F64 分支。

**Div 原生安全**：IEEE 浮点 /0 → ±inf/NaN（不 trap、不抛异常），与 interp 一致 → 可 native `fdiv`
（不像 i64 `sdiv` 必须留 helper 处理 catchable 异常）。**Rem 留 helper**（浮点取余是 `fmod` libcall、
非单指令）。

## Out of Scope（本 change 不做）

- **F32（single）原生化**：F32 widened 存 `Value::F64`，写回需 round 到 f32 精度，native `fadd` 不做 →
  F32 保留 helper（`is_f64_typed` 精确匹配 `F64`，排除 F32）。
- **混合 int/float**：`int + double` 由 helper 促 int→f64；native 路径要求三操作数全 F64（多数浮点热
  环是纯 double，覆盖主 case）。
- **F64 loop-carried 寄存器驻留（2C-for-floats）**：本 change 的 F64 op 直写内存（不进 2B 缓存/2C
  Variable，与 bool op 同）。F64 residency 是独立 follow-up。
- **Convert 到/从 float 的 native 化**（`fcvt_from_sint`/`fcvt_to_sint`）：独立 follow-up。

## 验证

- 正确性：`ftest.z42`（全 6 比较 + fadd/fsub/fmul/fdiv/fneg + **NaN/±inf/±0/inf×0 边界** + 混合
  int/float 留 helper）**interp==jit==jitOSR 逐字节**。
- 收益：纯 `double` 累加环 JIT **1.59×→2.78× interp**（jit 自身 608→349ms = 1.74×）。
- GREEN：`cargo --lib` + `xtask test e2e`（interp==jit 逐字节）+ 自举 5/5 gen1==gen2 + stdlib。
