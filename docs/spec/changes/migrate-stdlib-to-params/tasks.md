# Tasks: stdlib API 迁移到 `params`

> 归档 `add-params-varargs` 阶段 9 的收尾。前置 `stabilize-dispatch-keys`（方案 A，
> Path.Join/String.Join params）已合并 main。

- [ ] 1. 复核全库「限-arity 重载 / arg0/arg1」候选（确认仅剩 Concat/Format）
- [ ] 2. `String.Concat(a,b)` + `Concat(a,b,c)` → `Concat(params string[])`
- [ ] 3. `String.Format(fmt,arg0)` + `Format(fmt,arg0,arg1)` → `Format(fmt, params object[])`
- [ ] 4. 回归测试：Concat 0/1/2/3+ 参数 + Format 0/1/2/多占位符
- [ ] 5. 调用点核对：bench 两处 `String.Concat` expanded form 编通；无 Format 调用点
- [ ] 6. `xtask build stdlib` + `xtask test stdlib z42.core` 全绿（本地 two-gen 工具链）
- [ ] 7. 归档阶段 9 候选勾除 + ACTIVE.md 归还锁 + commit/push（GREEN 以 CI 为权威）

## 自举安全性核实（阶段 9 硬约束）

- z42c/xtask 不调用 `String.Concat`/`String.Format`（grep 零命中）→ 不破不动点。
- 无 wire 格式变化 → 不 bump zbc/zpkg。
