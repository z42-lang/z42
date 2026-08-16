# Proposal: JIT 原生整数除 / 取余（native sdiv/srem + 冷守卫）

> 状态：IMPL（vm 类，纯 runtime JIT codegen，无格式 bump、自举字节不动）。
> 承接 `jit-unbox-regalloc` + `jit-native-float*` 系列——整数原生化最后一块：`Div`/`Rem` 是唯一还全走
> helper 的常见算术。单逻辑单元、单 commit、单 PR。

## Why

`Add`/`Sub`/`Mul`/比较/位运算/转换/全 `double` 都已 native 后，整数 `Div`/`Rem` 仍每次调 `jit_div`/
`jit_rem` helper。留 helper 的历史原因是**硬件除法陷阱**：x86_64 `idiv` 对 `/0` 与 `i64::MIN / -1` 溢出
都触发 SIGFPE（硬崩进程），而 z42 语义要求 `/0` 抛可捕获的 `Std.DivideByZeroException`、`i64::MIN/-1`
按 Rust `x / y` panic。凡 div/rem 密集的热环每迭代白付一次 Rust call + 分派开销。

## What Changes（只动 `Div`/`Rem` 两个 codegen 臂）

新增局部宏 `emit_int_divrem!`（`jit/translate.rs`，紧邻 `check!`）：

- **冷守卫** `(b as u64).wrapping_add(1) <= 1`（一条 `iadd_imm` + 无符号 `icmp_imm`）当且仅当
  `b ∈ {0, -1}` 为真 → `brif` 到冷块调 helper：
  - `b==0` → helper 抛 `DivideByZeroException`（复刻 interp `check_int_div_by_zero`），`check!` 分派异常。
  - `b==-1` → helper 在 i64 宽度算 `x/-1`/`x%-1`，与 interp 逐位一致（含 `i64::MIN/-1` panic、
    `i32::MIN/-1` → i64 宽度 `2^31`）。`-1` 除数在热循环罕见，整值分流成本可忽略。
- **常路**（`b ∉ {0,-1}`）→ 原生 `sdiv`/`srem`，i64 宽度。窄整数全物理存 `Value::I64`，不做窄化，结果
  直存 `TAG_I64`——与 `emit_i64_binop` 同构，逐位复刻 interp 的 i64 `x/y`/`x%y`。

Div 臂优先级：`is_f64_typed` → native `fdiv`（既有）；否则 `is_int_typed` → `emit_int_divrem!`；否则
helper（混合 / 盒装）。Rem 臂：`is_int_typed` → `emit_int_divrem!`；否则 helper（含浮点 `%`）。

## 为什么无需碰 2C / 2B（正确性关键）

整数 `Div`/`Rem` 的 dst/a/b **早已**被 `compute_promotable_regs` 的 Div/Rem 臂 disqualify（不驻留
Variable），且不在 `instr_uses_int_cache`（执行前 `RegCache` 已 flush）——内存权威。冷块 helper 按 index
读写 `frame.regs` 天然一致；操作数/结果全走**直接内存**（`load_payload_i64` / `store_const_tag`），与内联
数组快路径的内存纪律同构。故 promotion 白名单与 cache 汇点**均不变**，本改是严格增量、零回归风险。

## Out of Scope

- **浮点 `%`**：`double % double` 非 int-typed → 落 helper 的 `int_binop_helper` f64 路径（`fmod` libcall）。
- **div/rem 操作数驻留**：这些 reg 依旧不进 2C（disqualified）——除数守卫引入的块切分与 2B 缓存不相容，
  且 divbench 的收益来自消除 helper 调用而非操作数驻留（循环计数器早已因 Div/Rem 而 disqualify，无回归）。
- **真无符号 U64 除法**：VM 全局对 U64 按有符号处理（同 compare/shift 现状）；native `sdiv`/`srem` 沿用
  有符号，与 interp/helper 一致。真无符号是独立 VM 级变更。

## 验证

- **正确性**（interp==jit==jitOSR 逐字节）：
  - `divcheck.z42`：sbyte/short/int/long 全宽度 × 正负操作数 × `-1` 除数（冷守卫）× JIT'd `/0` 捕获
    （d 循环过 0 → 冷块抛异常在 JIT 代码里被 catch）→ interp==jit==jitOSR(`Z42_OSR_THRESHOLD=1`)
    均 `433334763573340`。
  - e2e golden `int_divide_by_zero` 在 vm-jit-consistency 下守住 `/0` 可捕获性。
- **收益**（隔离，基 origin/main）：`divbench.z42`（`1000003/i + 1000003%i` 密集环）JIT
  **5.0×→8.87× interp**——div/rem helper→native 使 JIT 自身**快 1.76×**（jit 581→330ms），输出不变
  `3740431077600`。
- **GREEN**：`cargo --lib` 948/0 + `xtask test all`（e2e interp+vm-jit-consistency + stdlib + 自举
  5/5 gen1==gen2 逐字节）。纯 runtime，无格式 bump。
