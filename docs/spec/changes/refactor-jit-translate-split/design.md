# Design: refactor-jit-translate-split（translate.rs 拆分 + 解释器/JIT 语义三重实现收敛）

> 类型：refactor（纯重构，零外部行为变更）+ 新增差分测试。
> 对应 runtime_review.md 的 **H2(translate)** 与 **H3(语义三重实现)**，是 review 排序表第 4 项。

## 背景与目标

`src/runtime/src/jit/translate.rs` 现 2983 行（硬限 500），其中单个 `translate_function`
占 1770 行（192–1961）。同时，同一算术/比较/转换语义在**三处**各写一遍：

1. interp：`interp/ops.rs`（`int_binop`/`numeric_lt`/`eval_cmp`）+ `interp/exec_value.rs`（`convert_value`/`check_int_div_by_zero`）
2. JIT runtime helper：`jit/helpers/arith.rs` + `jit/helpers/mod.rs` 的 `int_binop_helper`/`numeric_lt_helper`（**与 interp/ops.rs 逐字重复**）+ `jit/helpers/object.rs::jit_convert`（**已直接委托 interp**）
3. JIT 内联 Cranelift：`translate.rs` 的 `emit_i64_binop`/`emit_i64_cmp`/`emit_f64_*`/`emit_i64_convert`/`emit_int_divrem` 等，`reg_types` 证明同型时绕过 helper 直发 Cranelift 原语

三处靠**手写注释**（"与 interp 对齐"、"matches interp::exec_value::check_int_div_by_zero"）维持一致；
历史上已发生并修过多次漂移（`fix-jit-int-div-by-zero` 2026-05-30、`fix-char-comparison` 2026-05-24）。
对「编译器全自举 byte-identical」目标，interp↔JIT 语义分歧是致命隐患。

**本变更目标**（零行为变更）：
- (H3) 把可共享的标量规则收敛到唯一真相源 `semantics.rs`，删除逐字重复；为不可共享的内联 Cranelift 路径建立**差分测试**把「注释担保」升级为「测试担保」。
- (H2) 把 `translate.rs` 拆成 `translate/` 子模块（镜像 `interp/exec_*` 组织），每文件 <500 行、分发函数体 <60 行。

## Architecture

### 现状（三份实现，注释耦合）
```
interp/ops.rs::int_binop ─┐
interp/exec_value.rs      │  (逐字重复)
   convert_value          │
jit/helpers/mod.rs        │
   int_binop_helper ──────┤  ← 逐字 copy of ops.rs
   numeric_lt_helper      │
jit/helpers/object.rs     │
   jit_convert ───────────┘  ← 已委托 interp::exec_value::convert_value
jit/translate.rs::emit_* ─── Cranelift 原语，靠注释"与 interp 对齐"
```

### 目标（单一真相源 + 内联锚定 + 差分测试）
```
              ┌──────────────────────────────────────┐
              │  src/runtime/src/semantics.rs         │  ← 唯一标量规则源
              │   int_binop / numeric_lt / eval_cmp   │
              │   convert_value / div_by_zero_check   │
              │   + 载重决策常量（OVERFLOW=Wrapping   │
              │     INT_DIV_ZERO=Throw …）            │
              └───────┬───────────────┬───────────────┘
        调用（运行期）│               │调用（运行期）
       interp/ops.rs ─┘               └─ jit/helpers/{arith,mod,object}.rs
                                          （删除重复，改为 call semantics::*）
              ┌───────────────────────────────────────┐
              │ jit/translate/*.rs  emit_* (Cranelift) │  ← 不能运行期调用 Rust；
              │   // SEMANTICS: semantics::<fn> 锚注释  │    以注释锚定 + 差分测试担保
              └───────────────────────────────────────┘
                        ▲
             semantics_jit_diff_tests.rs（新）：同一批边界输入
             跑「标量规则」vs「JIT 编译执行」，断言 Value 逐字节相等
```

## Decisions

### Decision 1: 语义真相源放 crate 顶层 `semantics.rs`，不放 interp/ 内
**问题**：共享标量规则该放哪？
**选项**：
- A：留在 `interp/ops.rs`，jit 依赖 interp（jit 已依赖 interp 一部分）
- B：新建 crate 顶层 `src/runtime/src/semantics.rs`，interp 与 jit 都依赖它
**决定**：选 B。标量规则（i64/f64/char 的算术·比较·转换）是**执行模型中立**的语言语义，不属于
interp 的执行循环，也不属于 jit。放顶层让两个后端**对称依赖**，避免「jit→interp」这种"借用另一个后端的内部"
的耦合方向，也为将来 AOT 复用同一真相源留位。`interp/ops.rs` 保留 interp 专属的
**寄存器访问包装**（`int_binop(regs, a, b, …)` 从寄存器取值后调 `semantics::int_binop(va, vb, …)`）。

### Decision 2: 三站点分两种收敛手段（能共享的共享，不能的锚定+测试）
**问题**：内联 Cranelift 路径无法运行期调用 Rust 标量函数，如何"收敛"？
**决定**：
- 站点 #1（interp）+ #2（jit helper）：**真共享**——都 `call semantics::*`，物理删除
  `helpers/mod.rs` 的 `int_binop_helper`/`numeric_lt_helper` 逐字重复、`arith.rs` 的 div-zero 重复。
- 站点 #3（jit 内联）：**锚定 + 差分测试**——每个 `emit_*` 加 `// SEMANTICS: semantics::<fn>`
  文档锚，`semantics.rs` 用**具名常量/枚举**记四个载重决策（见 Decision 3）使两侧引用同一出处；
  再加**差分测试**（本变更新增，当前完全缺失）跑边界输入断言两路 byte-identical。
**理由**：符合 review「内联路径只负责寄存器访问，计算规则用 semantics 的 Cranelift 等价物并以注释锚定」；
差分测试把 B.2 列出的 6 个漂移点从"注释担保"变"测试担保"，是本变更真正的一致性价值所在。

### Decision 3: 载重决策显式化为 semantics.rs 的具名常量
把散落在注释里的四个易漂移决策提升为**代码中的具名锚点**，两侧都引用：
| 决策 | 值 | 现状散落处 |
|------|----|-----------|
| 整数溢出策略 | `Wrapping`（add/sub/mul） | arith.rs:11 注释、exec_value.rs:60 注释 |
| 整数除零 | `Throw(DivideByZeroException)` | arith.rs:74-79、exec_value.rs:114 |
| 整数 div/rem `MIN/-1` 守卫 | `{0,-1} 路由到标量` | translate.rs:753-758 位技巧 |
| float→int | `SaturateNaNZero`（NaN→0，饱和） | exec_value.rs:320、translate.rs:2589 |
| int→float | `FullF64Precision`（F32 目标也走 f64） | exec_value.rs:353、translate.rs:2566 |
| 比较 | `SignedOrdered`（Ne 用 unordered NotEqual） | translate.rs:2793、2810 |

### Decision 4: 不支持指令清单收敛为单函数
`jit_unsupported_reason`（prescan，translate.rs:59-84）与 match 内 `bail!` 臂（1672-1751）
两处手工维护同一份"哪些指令 JIT 不支持 + 原因"。收敛为
`unsupported_reason(instr: &Instruction) -> Option<&'static str>`（须接收 `&Instruction`
而非静态数组——因 generic `Call`/`VCall` 是**条件**不支持，要看 `method_type_args`）。
prescan 循环调它；`bail!` 臂改为 `bail!(unsupported_reason(instr).expect("prescan invariant"))`
——message 文本同源，两检查点不可能漂移。**保留显式臂**（runtime-rust.md 禁 `_` 兜底）。

### Decision 5: translate.rs 拆分靠 TxCtx 上下文结构（非 impl 分割）
`translate.rs` 无 impl 块，1770 行 `translate_function` 的每个 match 臂闭包捕获大量局部可变状态
（`builder`/`regs_base`/`cache`/`promoted`/`hr_*` FuncRefs/`catch_chain`/`cl_blocks`…）。
引入 `pub(super) struct TxCtx<'a,'b>` 打包这些状态，6 个文件内 `macro_rules!`
（`ri!`/`str_val!`/`regs_val!`/`check!`/`emit_dispatch_to_catch_or_return!`/`emit_int_divrem!`）
转为 `impl TxCtx` 方法。每个 match 臂体变成 `fn translate_<op>(cx: &mut TxCtx, …)`，
驱动 `mod.rs` 保留**穷尽** dispatch match（每臂一行调子模块函数，镜像 `interp/exec_instr.rs:75`），
新增 opcode 仍在驱动处触发编译错误。`hr_*` 复用 `helpers/registry.rs::HelperIds` 打包传递。

## 模块拆分方案（H2）

`translate.rs` → `jit/translate/` 目录（`mod.rs` 为薄驱动）：

| 模块 | 承载 | ~行 |
|------|------|-----|
| `mod.rs`（驱动） | `translate_function` 外壳（签名 + 4 个 prescan + `hr_*` 绑定 + block 循环 + dispatch/terminator match 路由）+ `max_reg`/`compute_promotable_regs`/`instr_uses_int_cache` | ~450 |
| `ctx.rs` | `TxCtx` + 宏转方法 + `load_int`/`store_int`/`load_f64`/`store_f64` 寄存器访问 | ~300 |
| `arith.rs` | Add/Sub/Mul/Div/Rem/Neg + 位运算 + And/Or/Not 臂 + 整型谓词 | ~320 |
| `arith_emit.rs` | `emit_i64_binop`/`emit_i64_neg`/`emit_i64_bit_not`/`emit_bool_binop`/`emit_bool_not`/`emit_f64_binop`/`emit_f64_neg`（H3 锚定面） | ~180 |
| `compare.rs` | Eq/Ne/Lt/Le/Gt/Ge 臂 + `emit_i64_cmp`/`emit_f64_cmp` + cmp 谓词 | ~180 |
| `convert.rs` | Convert 臂 + `emit_i64_convert`/`emit_int_to_f64`/`emit_f64_to_int`/`is_typed` | ~260 |
| `value.rs` | Const*/Copy/StrConcat/ToStr 臂 + `emit_const_*`/`emit_primitive_copy`/`is_drop_free_primitive` | ~280 |
| `call.rs` | Call/Builtin/CallIndirect/LoadFn/LoadFnCached/MkClos + `method_id_at`/`call_jit_ic_ptr_at` | ~350 |
| `array.rs` | ArrayNew/ArrayNewLit/ArrayGet/ArraySet/ArrayLen + `arr_prim_elem`/`idx_int_ok` | ~260 |
| `object.rs` | ObjNew/Typeof/FieldGet/FieldSet/VCall/IsInstance/AsCast/StaticGet/StaticSet + field/vcall/static IC 辅助 | ~430 |
| `structs.rs` | StructAlloc/StructCopy/StructFieldGetPrim/StructFieldSetPrim/DefaultOf | ~60 |
| `control.rs` | terminator 臂（Ret/Br/BrCond/Throw）+ `emit_safepoint_check`/`find_handler_entries` + catch 分发方法 | ~280 |
| `unsupported.rs` | `unsupported_reason(&Instruction)` 单表 + prescan/bail 共用 | ~90 |

可见性：跨子模块调用的 `load_int`/`store_int` 等 + `TxCtx` 方法 + `BinopKind`/`CmpKind` 等枚举 → `pub(super)`；
仅本类别用的谓词随臂私有。

## Implementation Notes

- **实施顺序（两个逻辑 commit，一个 PR）**：
  1. **commit 1（H3 收敛）**：新建 `semantics.rs`；迁移 `int_binop`/`numeric_lt`/`eval_cmp`/`convert_value`/
     除零检查；interp/ops.rs + exec_value.rs 改调它；删 `helpers/mod.rs`/`arith.rs` 重复；
     `emit_*` 加 SEMANTICS 锚注释；新增 `unsupported_reason`；新增差分测试。**不动文件拆分。**
  2. **commit 2（H2 拆分）**：引入 `TxCtx`，`translate.rs` → `translate/` 12 子模块，纯机械搬移。
  先 H3 后 H2：H3 让 `emit_*` 先锚定好，拆分时连锚一起搬，避免拆后再回锚。
- **差分测试机制**：复用 `jit::lazy` 单测既有的"编译单函数并执行"设施（README 载 `cargo test --lib jit::lazy`）。
  边界输入表覆盖：`i64::MIN/-1` div、`/0`、`%0`、`NaN` 比较、`f64::MAX` 饱和、`T_U64` 边界、shift ≥64。
- **零行为变更验证**：现有 `xtask test e2e`（interp）+ CI `test-vm-jit`（JIT 逐字节对 interp）+ 自举
  `gen1==gen2` byte-identical 是主回归网；差分测试是新增的定点担保。

## Testing Strategy
- **差分测试（新）**：`jit/semantics_jit_diff_tests.rs`（或并入 `jit/translate/` 下），断言标量规则 vs JIT 编译执行 byte-identical。
- **回归（现有全绿门）**：`cargo build --release` + `xtask test`（e2e interp / cross-zpkg / stdlib / compiler 自举 / vscode-syntax）。
- **JIT 一致性**：本地 `xtask test e2e --mode jit`（输出须与 interp 逐字节一致）+ CI `test-vm-jit`。
- **自举定点**：`xtask test compiler` 的 gen1==gen2 byte-identical（interp↔JIT 语义分歧会在此暴露）。
- 纯 refactor，不新增 golden；差分测试 + 既有网覆盖。
