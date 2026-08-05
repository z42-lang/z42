# jit — Cranelift JIT backend

## 职责
将 z42 SSA 字节码编译为原生机器码执行。所有值操作通过 `extern "C"` helper 函数实现，Cranelift 只生成控制流（分支、跳转、函数入口/出口）。**编译是惰性的**：函数在**首次被调用**时才编译（compile-on-first-call），不在加载时全量编译——机制见 [book: 惰性逐函数 JIT](../../../../docs/book/src/runtime/jit-lazy-compile.md)。

## 核心文件
| 文件 | 职责 |
|------|------|
| `mod.rs` | 公开 API（`JitModule::setup` 建基础设施不编译、`JitModule::run` 先编入口再执行、`jit::run` 入口）；委托 helper 注册到 `helpers::registry` |
| `lazy.rs` | `LazyCompiler`：持 cranelift `JITModule` + helper ids；`setup`（建基础设施）+ `compile_one`（按需编译单函数）。Mutex 守护，`Z42_JIT_PROFILE` 逐函数打印 |
| `frame.rs` | `JitFrame`（寄存器文件 + 变量槽）、`JitModuleCtx`（`OnceLock` 编译槽 + 字符串池 + 集中解析器 `resolve_fn_by_id`/`resolve_fn_by_name`，热路径零锁、首编串行化） |
| `translate.rs` | `translate_function`（z42 指令 → Cranelift IR，取单 `FuncId`）；`HelperIds` 重导出自 `helpers` |
| `helpers/` | `extern "C"` helper 集合（按指令类别拆分；与 `interp/exec_*.rs` 命名对称）。查表统一经 `resolve_fn_*`（含惰性 hook） |

### `helpers/` 子目录
| 文件 | 职责 |
|------|------|
| `mod.rs` | 共享工具（`vm_ctx_ref` / `set_exception` / 数值 helper / `JitFn`）+ `VM_JIT_INTERFACE_VERSION` |
| `registry.rs` | **中央 helper 注册表**：`HelperIds` 结构、`register_symbols`（→ JITBuilder）、`declare_imports`（→ JITModule） |
| `value.rs` | 常量加载、Copy、字符串、`get_bool` / `set_ret` |
| `arith.rs` | 算术、比较、逻辑、一元、位运算 |
| `control.rs` | `throw` / `install_catch` / `match_catch_type` |
| `call.rs` | `jit_call`、`jit_builtin` |
| `array.rs` | 数组分配、元素访问、长度 |
| `object.rs` | 对象分配、字段访问、类型检查、静态字段、`default(T)` |
| `vcall.rs` | 虚调用（独立文件，含 primitive-as-struct + 懒加载 fallback） |
| `closure.rs` | L3 闭包：`load_fn` / `mk_clos` / `call_indirect` / `load_fn_cached` |

## 入口点
- `jit::run(ctx, module, entry)` → 建基础设施（`JitModule::setup`）+ 执行入口
- `JitModule::setup(module)` → `JitModule`（不编译任何用户函数）
- `JitModule::run(entry_name)` → 先编入口函数，再执行；其余函数首次调用时经 `resolve_fn_by_id` 惰性编译

## 如何测试验证
```bash
cargo test --manifest-path src/runtime/Cargo.toml --lib jit::lazy   # 惰性编译单测（8 个）
./xtask test e2e --mode jit                                         # golden 端到端（JIT，输出须与 interp 逐字节一致）
Z42_JIT_PROFILE=1 <z42vm> <artifact> <entry> --mode jit             # 打印每个被惰性编译的函数（数十，非整套 stdlib）
```

## Helper 边界（formalize-jit-vm-interface, 2026-05-07）

加新 helper 改 **2 处**:
1. 对应 `helpers/<category>.rs` 添加函数定义
2. `helpers/registry.rs` 添加 `register_symbols` 中的 `reg!()` 行 + `HelperIds` 字段 + `declare_imports` 中的 `decl!()` 行

详见 [docs/design/runtime/vm-architecture.md](../../../../docs/design/runtime/vm-architecture.md) "JIT/EE helper 边界"。

## 依赖关系
- 依赖 `corelib` 的 `exec_builtin` 和 `value_to_str`
- 依赖 `metadata` 的 `Module`、`Function`、`Instruction`、`Value` 等类型
- 依赖 `interp::primitive_class_name` + `interp::value_synthetic_type_id`（vcall 共享判定 / primitive-receiver IC 键）+ `interp::dispatch::is_subclass_or_eq_td`（control 共享）
- 外部依赖：`cranelift-codegen`、`cranelift-frontend`、`cranelift-jit`、`cranelift-module`
