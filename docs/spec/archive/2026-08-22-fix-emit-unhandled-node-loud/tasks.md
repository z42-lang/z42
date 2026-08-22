# Tasks: fix-emit-unhandled-node-loud

**变更说明：** `ExprEmitter.Emit` 的 dispatch 尾部，此前对**任何未处理的 `BoundExpr` 节点**静默
emit `const.null` 占位（注释「其余节点 → 后续增量」）。这是个隐蔽陷阱：**新增一种 `BoundExpr` 却漏接
emit dispatch 时，不报错，而是产出错误的 codegen（null 占位）**——最坏的一类 bug（静默错误字节）。
改为：`BoundError`（类型检查失败的错误恢复哨兵，仅错误路径可达）显式保留原 const.null 优雅降级；
其余任何未处理节点 → `throw new Exception("... unhandled BoundExpr kind: " + e.Dump())` 响亮报错。

**原因：** dispatch 穷尽性安全网。当前 34 个 `BoundExpr` 子类中仅 `BoundError` 未被 33 个 emit 分支
处理，而 `BoundError` 只在编译已失败的错误恢复路径出现（成功编译在 emit 前已因类型错误中止）→ 成功
编译路径上该 fallback 是**死代码** → 改为 throw **逐字节透明**（throw 对所有当前节点不可达）。价值在
**未来**：下一个新增 `BoundExpr` 若漏接 emit，立即崩溃带节点 Dump，而非静默产出错误字节。

**文档影响：** 无外部可见行为变更（仅内部错误路径从「静默错误」变「响亮崩溃」）；以源码头注承载理由。
属 emit dispatch 穷尽性的局部安全加固，是后续「Bound visitor dispatch」设计（见
`docs/spec/changes/refactor-bound-visitor-dispatch/`）的**独立前置小步**——不依赖它、可单独落地。

- [x] 1.1 `ExprEmitter.Emit` fallback：`BoundError` 显式 const.null 降级 + 其余 throw（带 `e.Dump()`）
- [x] 1.2 字节不动点守卫：modified vs clean 编出 **24/24 stdlib zpkg sha256 逐字节一致 ✅**
- [x] 1.3 完整 `xtask test` 全绿（self-host 5/5 gen1==gen2 + 全 stage）
