# 解释器 / JIT 标量语义的单一真相源

> 对齐：2026-08-23（change `refactor-jit-translate-split`，H3）。
> 代码：`src/runtime/src/semantics.rs`（真相源）、`src/runtime/src/interp/ops.rs` +
> `interp/exec_value.rs`（interp 消费）、`src/runtime/src/jit/helpers/arith.rs` +
> `jit/helpers/object.rs`（JIT helper 消费）、`src/runtime/src/jit/translate/emit_int.rs` +
> `emit_fc.rs`（JIT 内联镜像）。

## 问题：同一语义三处实现

z42 的算术 / 比较 / 数值转换语义，运行时有**三条执行路径**各自表述同一套标量规则：

1. **interp 执行循环** —— 按 `Value` 类型 match，走寄存器级包装（`ops.rs`）。
2. **JIT runtime helper** —— `extern "C"` 函数，再做一遍类型判断。
3. **JIT 内联 Cranelift** —— 当 `reg_types` 静态证明操作数同型（全 I64 / 全 F64）时，
   JIT **绕过 helper**，直接发 Cranelift 原语（`iadd`/`fcmp`/`fcvt_to_sint_sat`…）。

改一处语义（wrapping 策略、除零行为、新数值类型）需记得同步三处；漏改即 interp 与 JIT
行为分歧。对「编译器全自举 byte-identical」目标，这是致命隐患——历史上已发生并修过多次漂移
（`fix-jit-int-div-by-zero`、`fix-char-comparison`）。

## 机制：能共享的共享，不能的锚定 + 差分

### 路径 1 + 2：真共享（运行期同一 Rust 函数）

`semantics.rs`（crate 顶层，interp / jit / 未来 AOT **对称依赖**）持**值级标量规则**：

```
int_binop(va, vb, int_op, float_op)   numeric_lt(va, vb)   eval_cmp(op, va, vb)
int_bitop(va, vb, op)                 convert_value(v, to_tag)
is_int_div_by_zero(divisor)  DIV_BY_ZERO_EXC  div_by_zero_msg(op)  SHIFT_MASK
```

- interp `ops.rs` 保留**寄存器取值**（「undefined register」错误是 interp 执行模型专属），
  取到 `&Value` 后调 `semantics::*`。
- JIT helper（`arith.rs`）删除此前**逐字重复**的 `int_binop_helper`/`numeric_lt_helper`
  （曾是 `ops.rs` 的副本），改调 `semantics::*`；`jit_convert` 委托 `semantics::convert_value`。

于是路径 1、2 对同一规则只有**一份**实现。

### 路径 3：注释锚定 + 差分测试（无法运行期调 Rust）

内联路径发的是机器码，不能在运行期 `call` 一个 Rust 标量函数（那正是它要绕开的开销）。
故它以 **`// SEMANTICS: semantics::<fn>` 锚注释**引用对应规则，并由**边界 golden 差分测试**
（`src/tests/operators/arith_semantics_edge.z42`，在 interp 与 `--mode jit` 下各跑一遍、
断言 byte-identical）把「注释担保」升级为「测试担保」。

`semantics.rs` 模块文档以一张表钉住**六个易漂移的载重决策**，三路共同引用：

| 决策 | 规则 | 内联镜像 |
|------|------|---------|
| 整数 add/sub/mul 溢出 | wrapping | `emit_i64_binop`：`iadd`/`isub`/`imul` |
| 整数 div/rem 除零 | 抛 `Std.DivideByZeroException` | `emit_int_divrem`：冷路由 `b∈{0,-1}` |
| float→int | 饱和 + NaN→0；`U64` 按 signed i64 饱和 | `emit_f64_to_int`：`fcvt_to_sint_sat` |
| int→float | 全 f64 精度（F32 目标也走 f64） | `emit_int_to_f64`：`fcvt_from_sint` |
| 数值比较 | signed ordered；`Ne` unordered（`NaN!=NaN→true`） | `emit_i64_cmp`/`emit_f64_cmp` |
| 整数移位量 | mask 到低 6 位（`& SHIFT_MASK`） | `emit_i64_binop`：`Shl`/`Shr` 前 `band 63` |

> **为何 `MIN/-1` 守卫只在内联路径**：native `idiv` 在 x86-64 对 `i64::MIN / -1` 会 SIGFPE-trap，
> 故内联用 `(b as u64).wrapping_add(1) <= 1` 判 `b∈{0,-1}` 冷路由到 helper（与 interp 的 i64
> `x/y` 语义一致）。这是唯一的守卫点，差分测试专门钉住 `MIN/-1`、`/0`、`%0`。

## 配套：JIT 不支持指令单表

「哪些 opcode JIT 不能翻译、原因是什么」此前在两处手工并行维护（prescan `jit_unsupported_reason`
与 translate 的 `bail!` 臂）。收敛为单个 `unsupported_reason(&Instruction) -> Option<&str>`
（`jit/translate/unsupported.rs`）：prescan 循环调它；每个 `bail!` 臂的原因文案也源自它，两检查点
不可能漂移。须收 `&Instruction`（非静态 opcode 集）——generic `Call`/`VCall` 是**条件**不支持。

## 相关

- JIT 内联快路径与 helper 边界：[JIT 惰性逐函数编译](jit-lazy-compile.md)、
  `src/runtime/src/jit/README.md`。
- 引入：change `refactor-jit-translate-split`（`docs/spec/archive/`）。
