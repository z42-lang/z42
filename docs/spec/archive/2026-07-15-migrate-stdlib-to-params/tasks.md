# Tasks: stdlib API 迁移到 `params`

> 状态：🟢 已完成 | 完成：2026-07-15 | 类型：stdlib follow-up（add-params-varargs 阶段 9 收尾）
> 前置 `stabilize-dispatch-keys`（方案 A，Path.Join/String.Join params）已合并 main。

- [x] 1. 复核全库「限-arity 重载 / arg0/arg1」候选（确认仅剩 Concat/Format）
- [x] 2. `String.Concat(a,b)` + `Concat(a,b,c)` → `Concat(params string[])`
- [x] 3. `String.Format(fmt,arg0)` + `Format(fmt,arg0,arg1)` → `Format(fmt, params object[])`
- [x] 4. 顺带删死别名 `Path.Combine`（0 调用点，`Path.Join` 286 处）
- [x] 5. 回归测试 `src/tests/strings/string_params_methods`（1/N 参 + normal-form 数组 +
      混类型装箱 + 重复占位符；避开空 params 退化点）
- [x] 6. 调用点核对：bench 两处 `String.Concat` expanded form 编通；无 Format 调用点
- [x] 7. clean 0.32 工具链验证：`test e2e --dir strings` 20/20（interp+jit）
- [x] 8. 归档阶段 9 候选勾除 + roadmap Deferred 登记空 params 缺陷

## 自举安全性核实（阶段 9 硬约束）

- z42c/xtask 不调用 `String.Concat`/`String.Format`（grep 零命中）→ 不破自举不动点。
- 无 wire 格式变化 → 不 bump zbc/zpkg（origin/main 与本地均 zpkg 0.32 / zbc 1.27）。

## 备注：发现独立编译器缺陷（越界，已 Deferred）

`string.Concat()` 零实参在某些 codegen 上下文崩溃（`undefined register`）——`_withParamsExpansion`
合成空数组字面量的 codegen/VM 缺陷。**非本变更引入、非 stdlib**（`compiler`/`runtime` 子系统），
边界见 proposal「已知限制」+ roadmap `params-future-empty-array-codegen`。回归用例避开该退化点。

## GREEN 权威

本变更 stdlib-only 且不 bump 格式；本地 clean 工具链 `test e2e --dir strings` 全绿，完整
GREEN gate（跨平台 / 冷环境自举链）以 CI 为权威。
