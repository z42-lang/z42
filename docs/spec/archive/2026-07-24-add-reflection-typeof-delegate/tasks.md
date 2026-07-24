# Tasks: typeof(委托) 直达句柄（typeof delegate handle）

> 状态：🟢 已完成 | 创建：2026-07-24 | 完成：2026-07-24 | 分支：feat/reflection-typeof-delegate（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；纯 z42c.semantics 内部，闭 add-delegate-metadata 的 typeof-delegate 剩余项）

**变更说明：** `typeof(MyDelegate)` 此前不达 delegate 句柄——delegate 名经 `ResolveType` 解析为结构化
`Z42FuncType`（`Name()` 返 `"Action"`/`"Func<…>"`），FQ delegate 名丢失，运行期解析不到 delegate 的
TYPE 条目。现 `typeof(MyDelegate).IsDelegate==true`、Invoke 可反射，与 `Type.GetType("<fq>")` 一致。

**原因：** add-delegate-metadata（2026-07-11）emit 了 delegate TYPE 条目（under FQ 名 + bit6），但
typeof 发射路径未对 Z42FuncType 特判 → 只能用 `Type.GetType("<fq>")` workaround。

**修复（纯 z42c.semantics，镜像 is/as 用 AST 原始名的先例）：**
- `Bound.z42` `BoundTypeof`：加 `TargetName`（AST 原始类型名）。
- `ExprTyper.z42` `_bindTypeofExpr`：从 `tox.Type`（`NamedType`）捕获 `TargetName`。
- `ExprEmitter.z42` `_emitTypeof`：对 `Z42FuncType` 目标用 `QualifyClass(TargetName)` 回 FQ delegate 名；
  非 delegate 走既有 `_typeofName`（gated，不影响其它 typeof）。

**无 z42.ir/格式/runtime 改动、无 bootstrap 影响。**

**文档影响：** `docs/design/language/reflection.md`（`reflection-future-typeof-delegate` 标记落地）。

- [x] 1.1 `Bound.z42`：`BoundTypeof.TargetName`
- [x] 1.2 `ExprTyper.z42`：`_bindTypeofExpr` 捕获 AST 名
- [x] 1.3 `ExprEmitter.z42`：`_emitTypeof` 对 Z42FuncType 用 QualifyClass(TargetName)
- [x] 1.4 `src/tests/types/typeof_delegate.z42`：e2e（IsDelegate / Name / 匹配 GetType / Invoke 可见 / 非委托不变）——interp+jit 空输出 exit0
- [x] 1.5 全绿：types e2e **76 pass 0 fail**（typeof/type_flags 无回归）+ stdlib z42.core **44 pass 0 fail**（test_delegate_metadata ✓）+ **compiler 自举不动点 5/5 gen1==gen2 byte-identical**
- [x] 1.6 `docs/design/language/reflection.md` 标记落地
- [x] 1.7 归档 + PR

## 备注
- 自举风险：改了 typeof 发射，但仅 Z42FuncType 目标受影响；z42c 源不 typeof(delegate) → gen1==gen2 byte-identical 预期成立（以 test compiler 为准）。
- 剩余：泛型 delegate（走 Z42InstantiatedType 分支）/ 匿名 func 类型（TargetName 空回落结构名）未特验。
