# Tasks: interp vcall vtable 路径去掉每次 arg Vec 分配

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06
**变更说明：** interp `exec_vcall::vcall` 的 object vtable 派发路径改用池化、无 Vec 的
`exec_function_from_receiver_regs`（镜像同函数的 IC 快路径 + JIT `jit_vcall`），不再
每次 `collect_args`（arg Vec）+ `vec![obj_val]`（receiver Vec）。`collect_args` 下沉进
primitive/array 路径（那里确实要把 `&[Value]` 交给 `exec_function`）。
**原因：** 消除 lazy 查找克隆（PR #130）后重新 profile z42c 自编译，interp 侧 malloc
**最大单一来源变成 `exec_vcall::vcall`（172 采样）**——object vcall miss IC 时每次分配
2 个 Vec。z42c 的 visitor 派发是 megamorphic（>4 类型，4-slot PIC 溢出 → 每次 miss →
每次走 vtable → 每次 2 次分配）。IC 快路径与 JIT 侧早已用池化帧无 Vec，本次把 interp
vtable 路径补齐到对称。
**文档影响：** 无对外行为变更（结果不变、语义等价）；纯内部分配优化。

## 机制
- 删 `let mut extra_args = collect_args(...)`（原在 primitive+vtable 之前无条件分配）。
- primitive/array 块内局部 `collect_args` + 构造 call_args（仅此路径需要 Vec）。
- vtable 路径删 `vec![obj_val] + append`；调用点 `callee_module_idx` / `callee_lazy` 改
  `exec_function_from_receiver_regs(ctx, module, callee, &obj_val, &frame.regs, args)`
  ——池化帧、receiver 入 reg0、args 从 caller regs 直填，与 IC 快路径逐字一致。
- 对 out/ref（`Value::Ref(Stack)`）等的处理与 IC 快路径同源（`new_from_receiver_regs`）。

## 任务
- [x] 1.1 `interp/exec_vcall.rs`：collect_args 下沉进 primitive 块；删无条件 extra_args
- [x] 1.2 `interp/exec_vcall.rs`：vtable 路径删 call_args；调用点改 from_receiver_regs
- [x] 1.3 测量：z42c 编译 big.z42 应较 #130 基线（interp 7.9s / jit 8.0s）再降
- [x] 1.4 GREEN：`cargo test --lib` 全过；e2e dir-golden 双模式 interp==jit==expected；
      big.z42 结果 interp==jit==159600、dict 正确
- [x] 1.5 megamorphic/多态派发回归：05_polymorphic / cross-zpkg vcall 用例结果不变

## Out of scope
- JIT 侧（已用池化帧，无需改）；primitive/boxed 路径（结构上需 Vec，保留）
- is_subclass_or_eq_td 的 String 分配（独立、更小的杠杆，后续）

## 备注
- 语义严格等价：from_receiver_regs 产出的 callee 帧布局（reg0=receiver, reg1+=args）
  与旧 `exec_function(&[receiver, ...args])` 完全相同；IC 快路径已证此路径正确。
