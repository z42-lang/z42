# interp — Tree-walking bytecode interpreter

## 职责
执行 IR 指令的解释器后端。逐块遍历、逐指令 dispatch，支持异常处理和虚方法分发。

## 核心文件
| 文件 | 职责 |
|------|------|
| `mod.rs` | 公开 API（`run`/`run_with_static_init`）、`Frame`（寄存器文件按 `Function::reg_file_len` 一次性预分配——loader 回填 `func.max_reg`，见 book「超级指令融合」页）、执行循环、异常表查找 |
| `exec_instr.rs` | 薄分发器：穷尽 match 把 `Instruction` 分派到下面 7 个 `exec_<category>.rs` |
| `exec_value.rs` | 常量 / Copy / 算术 / 比较 / 逻辑 / 一元 / 位运算 / 字符串构造 |
| `exec_address.rs` | `LoadLocalAddr` / `LoadElemAddr` / `LoadFieldAddr` / `DefaultOf` |
| `exec_call.rs` | `Call` / `Builtin` / `LoadFn` / `LoadFnCached` / `CallIndirect` / `MkClos` |
| `exec_array.rs` | `ArrayNew` / `ArrayNewLit` / `ArrayGet` / `ArraySet` / `ArrayLen` |
| `exec_object.rs` | `ObjNew` / `FieldGet` / `FieldSet` / `IsInstance` / `AsCast` / `Static*` |
| `exec_vcall.rs` | `VCall` + `primitive_class_name` + `is_array_isa`（独占文件因体积较大） |
| `exec_native.rs` | `CallNative` / `CallNativeVtable` / `PinPtr` / `UnpinPtr` |
| `dispatch.rs` | 对象分发辅助：vtable 解析、ToString 协议、子类检查、静态字段、fallback TypeDesc |
| `ops.rs` | 寄存器级辅助：`int_binop`、`numeric_lt`、`collect_args`、`bool_val`、`str_val` |

## 入口点
- `interp::run(module, func, args)` — 执行单个函数
- `interp::run_with_static_init(module, func)` — 初始化静态字段后执行

## 依赖关系
- 依赖 `corelib` 模块的 `exec_builtin` 和 `value_to_str`
- 依赖 `metadata` 模块的 `Module`、`Function`、`Instruction`、`Value` 等类型
