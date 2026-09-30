# Tasks: 类型代换统一为一份完整递归（unify-type-subst）

> 状态：🟢 已完成 | 创建：2026-09-30 | 类型：fix（最小化模式；源自 2026-09-30 z42c 审查「统一的类型代换器」）

**变更说明**：语义层有 5 份手抄的型参代换递归，只在「叶子怎么换」上不同，递归面各抄各的、已漂移：

| 函数 | 叶子规则 | 漏下钻 |
|---|---|---|
| `MemberResolver._substGeneric` | 类级型参（按 Def 下标，未命中 → Unknown）| 接口实例、函数类型 |
| `MemberResolver._substIfaceArgs` | 接口型参（按名）| 函数类型 |
| `MemberResolver._substSelf` | `Self` | 接口实例 |
| `MethodTypeArgSubst.ByName` | 方法级型参（按名）| 接口实例 |
| `InheritanceResolver._substForIface` | `Self` + 接口型参 | （完整）|

**原因（实测）**：泛型类成员返回 `IGet<T>` / `Func<T,int>` / `Func<int,T>` 时 `T` 漏给调用方，两个方向都错：
- 漏报：`string s = new Box<int>(3).AsGet().Get();` **零诊断编过**，运行期把 int 塞进 string；`b.Measure()("x")` 接受错误实参；
- 误报：`b.AsGet().Get() + 1`、`b.Maker()(1) + 1` 报 `E0402 operator + requires numeric operand, got T`；
- 接口静态类型同理：`IMk<int>.Mk()`（返回 `Func<int,T>`）`+ 1` 误报、赋给 string 漏报。

**文档影响**：internals `generics.md`「布局层的代换必须与语义层同口径」表的语义层一行。

## 任务
- [x] 1.1 新增 `z42c.semantics/src/TypeSubst.z42`：`TypeSubst.Apply` / `ApplySig`（型参 / 数组 / 类实例 / 接口实例 / 函数类型，
      函数类型搬运 ParamsFrom / ParamDefaults / ParamCallers / ParamNames）+ 叶子规则 `ITypeParamMap`：
      `ClassArgsMap` / `IfaceArgsMap` / `SelfMap` / `NameListMap` / `IfaceImplMap`（各自逐字保留原叶子语义）
- [x] 1.2 五个旧函数改为薄封装（名字不变，调用点零改动）；删 `_paramIndex`
- [x] 1.3 `CompilerFingerprint.Entries` 追加 `unify-type-subst`（既发新诊断又改推断类型 ⇒ 旧缓存条目必须作废）
- [x] 2.1 e2e `src/tests/generics/member_type_subst_nested.z42`（+ `.opt_all`）：接口实例 / Func 返回位 / Func 形参位 /
      数组→接口实例四种形状（**修复前红**：旧编译器 3 条 `got T` 误报）
- [x] 2.2 `typecheck/type_subst_coverage_tests.z42`（6）：漏报 ×3（类成员接口实例 / 类成员 Func 形参 / 接口成员 Func 返回）、
      误报 ×3
- [x] 3.1 文档：generics.md
- [x] 3.2 `xtask test` 全绿

## 备注
- `ClassArgsMap` 对非类形参名（方法级型参 U）仍返回 Unknown（旧 GS6 松绑行为），现在这条规则也作用于接口实例 / 函数类型内部——
  与原本对类实例实参的处理一致。
