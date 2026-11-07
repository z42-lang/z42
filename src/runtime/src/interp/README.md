# interp — 字节码解释器

## 职责
执行 IR 指令的解释器后端。逐块遍历、逐指令 dispatch，支持异常处理和虚方法分发。

## 核心文件

| 目录 / 文件 | 职责 |
|------|------|
| `mod.rs` | 后端模块表与公开执行 API |
| `engine.rs` | 注册到 VM 的基线 Engine，将参数视图交给 `run` 构造激活帧 |
| `run/mod.rs` | 基本块与指令主循环、分支、返回、safepoint 与 OSR 交接 |
| `run/entry.rs` | 公开执行入口与静态初始化入口 |
| `run/exec_support.rs` | 帧登记守卫、callee 帧入口、原生分流、异常事件及 ref 写回 |
| `run/exec_body_cold.rs` | 参数 copy-in、异常回卷、错误位置与标签跳转 |
| `ops/mod.rs` | 寄存器操作、参数视图、标量求值与错误映射 |
| `ops/exec_instr.rs` | 穷尽指令分派 |
| `ops/exec_value.rs` / `exec_address.rs` | 常量、算术、比较、字符串、地址和泛型默认值适配 |
| `ops/exec_call.rs` / `exec_vcall.rs` | 直接、间接、闭包与虚调用的寄存器适配 |
| `ops/exec_array.rs` / `exec_object.rs` / `exec_struct.rs` | 数组、对象、静态字段和值 struct 的寄存器适配 |
| `ops/exec_object_isa.rs` | 类型判定与 cast 适配 |
| `ops/exec_ffi.rs` | 原生调用与 pin 指令适配 |
| `frame/mod.rs` | 解释器寄存器帧、方法类型实参和 ref 写回记录 |

各实现的测试与被测代码同目录。帧栈、寄存器池和 arena 归 VM 的 `exec/call`；指令共享语义归 `vm/exec/ops`；融合计划由 `vm/runtime/fusion.rs` 准备。

## 入口点
- `interp::run(module, func, args)` — 执行单个函数
- `interp::run_with_static_init(module, func)` — 初始化静态字段后执行

## 依赖关系
- builtin 经 `vm/exec/call/builtin_abi` 调用；纯值格式化直接依赖 `object/value`
- `metadata` 提供静态 IR，`vm/runtime` 提供链接后的 `Module` / `Function` 与执行计划，`object` 提供 `Value`。
- 标量运算调用 `vm/exec/ops/scalar`；融合识别由 `vm/runtime/fusion` 准备。

## 如何测试验证

```bash
./xtask test runtime              # cargo test（含 interp 单测）
./xtask test e2e --mode interp   # golden 端到端（解释器）
```

## 关联文档

- [解释器与 JIT 语义](../../../../docs/internals/src/runtime/interp-jit-semantics.md)
- [超级指令融合](../../../../docs/internals/src/runtime/superinstr-fusion.md)
