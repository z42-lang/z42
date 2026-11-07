# interp — 字节码解释器

## 职责
执行 IR 指令的解释器后端。逐块遍历、逐指令 dispatch，支持异常处理和虚方法分发。

## 核心文件
| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块表 + 执行主循环 `exec_function_body`（入口 / Frame / 支撑函数经 `pub use` / `pub(crate) use` 全量再导出，兄弟模块 `super::X` 路径不变）|
| `entry.rs` | 公开执行入口（结果类型为 VM 的 `ExecOutcome`）：`run` / `run_returning` / `run_outcome`、`init_static_fields` / `run_with_static_init` |
| `frame.rs` | `Frame`（寄存器文件按 `Function::reg_file_len` 一次性预分配——loader 回填 `func.max_reg`，见 book「超级指令融合」页；`Frame.method_type_args` 携泛型方法调用点的具体类型实参名，供 `MethodTypeArg`/`MethodDefault`——见 book「泛型方法」页）、执行循环、异常表查找
| `exec_support.rs` | `exec_function` 族入口、`enter_frame` + `FrameGuard`（interp 帧登记 / RAII 弹出并归还寄存器文件）、OSR / 原生分流（`try_osr` / `try_native_exec`）、异常事件、`find_handler`、ref 写回 |
| `exec_instr.rs` | 薄分发器：穷尽 match 把 `Instruction` 分派到下面各 `exec_<category>.rs`；内联进执行主循环，大而少见的分支调 `#[inline(never)]` helper（「热循环布局」，见文件头注释）|
| `exec_body_cold.rs` | 执行主循环的冷路径（`ref` 参数 copy-in、异常回卷到本帧 handler、内部错误定位、按标签跳转），不占热循环的寄存器 / 栈帧 |
| `exec_value.rs` | 常量 / Copy / 算术 / 比较 / 逻辑 / 一元 / 位运算 / 字符串构造 |
| `exec_address.rs` | Typeof 经 VM 对象协议物化反射类型；`LoadLocalAddr` / `LoadElemAddr` / `LoadFieldAddr`（检查走 objops）/ `DefaultOf`（类级泛型零值）/ `MethodTypeArg`·`MethodDefault`（方法级泛型：读 `Frame.method_type_args`，解析和默认值操作与 JIT 共用，见 book「泛型方法」页）|
| `exec_call.rs` | `Call` / `Builtin` / `LoadFn` / `CallIndirect` / `MkClos`；`[Native]` 包装函数的折叠调用（`try_fold`） |
| `vm/exec/call/forward.rs`（共享） | **`[Native]` 桩折叠单一实现**（识别转发函数、`Function::forward` 缓存、无帧 / 轻帧执行、失败时现算包装函数的 trace 行），interp `exec_call` / `exec_vcall` 与 JIT `jit/compile/lookup.rs` / `jit/runtime/forward.rs` 共用；机制见 internals「native ABI §6.4」 |
| `exec_array.rs` | `ArrayNew` / `ArrayNewLit` / `ArrayGet` / `ArraySet` / `ArrayLen`：`objops::array` 的薄适配（另解析泛型型参的基元零值） |
| `exec_object.rs` | `ObjNew`；`FieldGet` / `FieldSet` / `Static*` 是 `objops::field` / `objops::statics` 的薄适配 |
| `exec_object_isa.rs` | `IsInstance` / `AsCast` 类型判定（`exec_object` 再导出） |
| `exec_struct.rs` | 值 struct 指令的寄存器薄适配，共享值级实现位于 `vm/exec/ops/structs.rs`：`StructAlloc` / `StructCopy` / `StructFieldGetPrim` / `StructFieldSetPrim`（作用于 `struct_arena`，寄存器持 `Value::StructRef` 句柄）；叶子读写核心在 `objops::struct_leaf` |
| `vm/exec/ops/vcall.rs` | **虚调用目标解析单一实现**（装箱基元 / 装箱 struct / 基元 / 对象 vtable·层级 walk + 候选名 + PIC 安装），interp `exec_vcall` 与 JIT `helpers/vcall.rs` 共用|
| `vm/exec/ops/obj_new.rs` | **`ObjNew` 类 / 构造器解析单一实现** + 站点缓存（`ObjNewSite`：类描述符 + ctor `FnId` + ctorless 标记，命中不哈希、不拿锁），interp `exec_object::obj_new` 与 JIT `helpers/object.rs::jit_obj_new` 共用 |
| `exec_vcall.rs` | `VCall` 的 interp 调用侧（PIC 命中 / 解析结果 → 帧执行、mixed-mode 原生分流）+ `primitive_class_name` |
| `exec_ffi.rs` | `CallNative` / `CallNativeVtable` / `PinPtr` / `UnpinPtr` 的寄存器适配与异常编排；调用和 owned pin 经 VM native 提供方 |
| `engine.rs` | VM 注册的基线 Engine；借用参数视图转入本后端原有 callee 帧构造入口 |
| `vm/runtime/fusion.rs`（共享） | 加载期融合识别规则与缓存形态；本后端只执行计划 |
| `vm/exec/ops/references.rs`（共享） | ref/out/in 位置读取与写回；帧只保留寄存器适配和 copy-in/copy-out 记录 |
| `ops.rs` | 寄存器级辅助：`int_binop`、`collect_args`、`bool_val`、`str_val` |
| `vm/exec/call/`（共享状态） | 帧栈、寄存器池与 stack / struct / transient arena 由 VM 执行层持有；解释器仅使用每个 VmContext 的实例，随帧 LIFO 回收并经统一栈面扫描根 |

## 入口点
- `interp::run(module, func, args)` — 执行单个函数
- `interp::run_with_static_init(module, func)` — 初始化静态字段后执行

## 依赖关系
- 依赖 `corelib` 模块的 `invoke_builtin` / `abi::error::raise`；纯值格式化直接依赖 `object/value`
- `metadata` 提供静态 IR，`vm/runtime` 提供链接后的 `Module` / `Function` 与执行计划，`object` 提供 `Value`。
- 标量运算调用 `vm/exec/ops/scalar`；融合识别留在本后端。

## 如何测试验证

```bash
./xtask test runtime              # cargo test（含 interp 单测）
./xtask test e2e --mode interp   # golden 端到端（解释器）
```

## 关联文档

- [解释器与 JIT 语义](../../../../docs/internals/src/runtime/interp-jit-semantics.md)
- [超级指令融合](../../../../docs/internals/src/runtime/superinstr-fusion.md)
