# Tasks: add-call-arity-diagnostics

> 状态：🟢 已完成（User 2026-09-15 确认 DRAFT）｜ 创建：2026-09-15 ｜ 来源：#652 登记的编译器缺口 ①

- [x] 1.1 普查：6+3 个探针挂全仓（compiler + stdlib + scripts + 全部 fixture），GREEN 状态下两轮；结论见 proposal「普查」表（违例仅 known-broken `examples/exceptions.z42` 一处）
- [x] 2.1 `OverloadBinder._withDefaults` 加 `staticForm`：静态形式调实例方法只接受「形参数 + 1」；同包 `_adaptArgs` 补不齐 → E1005（`_firstMissingRequired` 点名缺的形参）；跨包缺位无 `$Default`/caller 宏 → E1005（`_crossPkgMissingRequired`）；多传 → E1006；一次调用最多一条
- [x] 2.2 发码收敛到 `OverloadBinder.ReportArity`（字面量发码）；`DiagnosticCodes.TooManyArguments = "E1006"` + core 单测
- [x] 2.3 旁路汇入：同类无限定静态调用、`ns.func()` 改走 `_withDefaults`；局部函数 `_checkLocalFnArity`；跨包构造器补位前同一判据
- [x] 3.1 单测 `z42c.semantics/tests/typecheck/call_arity`（13 阳性 + 4 对照）；跨包「有默认值不误报」对照由 e2e `param_default_cross_pkg` / `crosspkg_ctor_default` 承担（`ExtractExports` 不带 `$Default`，单测造不出）
- [x] 3.2 退回对照：撤掉三处判定源码、保留单测 → 13 阳性全 FAIL、4 对照 PASS
- [x] 4.1 文档：book `compiler/error-codes.md` E1005/E1006；`examples-known-broken.txt` 原因栏补 `int.TryParse(s, out var n)`
- [x] 5.1 冷种子 GREEN（nightly SDK → build ×2 → `xtask test` 全绿、不动点 3/3）+ `test e2e --dir cross-zpkg --mode jit` 53/53 + 全量 `cargo test` 1378；字节对账：stdlib 仅 `z42c.core` 变（源码改动所致）
