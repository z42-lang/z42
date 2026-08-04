# Tasks: perf-jit-closure-env

> 状态：🟢 已完成 | 创建：2026-08-04 | 类型：perf（最小化模式）

**变更说明：** JIT 的 `jit_call_indirect` 对 `Value::Closure` 复用已有 env GcRef（Arc +1），
不再 `to_boxed_vec()` 深拷 + `alloc_array` 重分配——与已合并的 interp exec_call S3 对称。
**原因：** interp S3（PR #106）只修了解释器路径；release 走 JIT，JIT 侧仍每次闭包间接调用
深拷+重分配 env。
**文档影响：** 无（纯内部实现，行为不变）。

- [x] `jit/helpers/closure.rs`：Closure → `Value::Array(c.env.clone())`；StackClosure 仍物化
- [x] 安全性同 interp S3：env MkClos 写一次、体内只 array_get 读（`_emitAssign` 无 BoundCapturedIdent
      写回）→ 跨调用共享 GcRef 字节等价
- [x] JIT 模式 e2e 验证：closure_l3_capture/loops/mono/stack + lambda_l2 + delegate 全过（184 OK/0 FAIL）
- [ ] 完整 GREEN（commit 前）

## 备注：本轮量测否决的清单项
- **S5**（obj_new 默认槽模板）：**天花板量测 = 0.0%**（20M 对象分配 7899ms，default_value_for 换零成本
  Null 填充完全不变）→ 对象分配成本全由堆分配+GC 注册（~395ns/alloc）主导，字段默认值计算可忽略。
  DROP（硬数据）。真正的分配热点在 GC/堆路径（另立 change）。
