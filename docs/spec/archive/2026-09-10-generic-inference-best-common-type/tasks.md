# Tasks: 泛型推断的数值最佳公共类型

> 状态：🟢 已完成（2026-09-10）｜ scope: compiler（无格式 bump）

| # | 阶段 | 状态 |
|---|---|---|
| 0 | 在今天 main 上重新核实 gap（勿信 Deferred 旧根因） | 🟢 |
| 1 | `_unify` 冲突分支取 `ArithmeticResult`（仅数值） | 🟢 |
| 2 | 负例/阳性门 + 退回对照 | 🟢 |
| 3 | 文档同步（generics.md §类型实参推断） | 🟢 |
| 4 | 完整 GREEN + 自举不动点 | 🟢 |

## 阶段 0 —— 重新核实（勿重跑）

- [x] 0.1 确认 gap 仍在：`TypeArgInference._unify:113` 冲突即 `false` → `Infer` Ok=false → 静默。
- [x] 0.2 确认 #546（398c34d8）正交：它归一**单个**已推绑定（`_resolvedForm`），不碰冲突路径。
- [x] 0.3 找到可复用件：`TypeFacts.ArithmeticResult`（`BinaryTypeTable.z42:92`）+ `TypeFacts.IsNumeric`。

## 阶段 1 —— 实现

- [x] 1.1 `_unify` 的 `Z42GenericParamType` 分支：`CanonName` 不等且双方 `IsNumeric` → 写回
      `ArithmeticResult(bound[idx], a)` 返回 true；否则 return false（边界 D2 静默）。

## 阶段 2 —— 门 + 退回对照

- [x] 2.1 阳性门 `test_numeric_conflict_infers_common_type_and_checks_where`：`where T:IFoo` + `p(1,2L)`
      → 修后 T=long、long 不满足 IFoo → 1 个 E0402（distinguishes 0→1）。
- [x] 2.2 拓宽方向门 `test_numeric_conflict_widens_to_long_no_false_error`：`p(1,2L)` 无约束 → 0 E0402
      （long 接受 int/long 两实参；抓「合错方向」）。
- [x] 2.3 保留 `test_conflicting_bindings_fail_inference_silently`（string+int → 仍 0，D2 不变），更正其注释。
- [x] 2.4 退回对照：stash `_unify` 一处、重建 → 阳性门 FAIL（0≠1，`values not equal`），证明是真门；恢复。

## 阶段 3 —— 文档

- [x] 3.1 `generics.md` §类型实参推断：更正「v1 不做最佳公共类型」→ 新增「数值冲突取拓宽公共类型」段
      + 边界（非数值仍静默失败）+ 对齐头。

## 阶段 4 —— GREEN

- [x] 4.1 `xtask test compiler`：generic_inference 三门全 PASS。
- [x] 4.2 完整 GREEN（build wave + stdlib + compiler 自举不动点）。
