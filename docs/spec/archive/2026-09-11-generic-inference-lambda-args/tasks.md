# Tasks: generic-inference-lambda-args

> 🔴=未开始 🟡=进行中 🟢=完成 ｜ proposal/design 见同目录

## 实现

- 🟢 T1 `MethodTypeArgSubst.SubstituteFuncParams`：只对 `Z42FuncType` 形参位 `ByName` 代换（只换已绑
  非 null 的型参名），非-Func 位原样、返回位不换（B2）。
- 🟢 T2 `TypeArgInference.InferPreBinding`：源①（非-null 已绑位 unify Type()）+ 源②（null 位且 rawArgs
  是 LambdaExpr 且形参 Z42FuncType → 用标注形参类型 unify）。返回部分绑定（未绑=null，冲突容忍）。
- 🟢 T3 注入点选定为 `OverloadBinder._withDefaults` 首行（新 `_inferFuncParamSubst`）——**5 处调用点的
  公共汇聚步**，一处覆盖自由函数 / 静态 Class.m / ns-限定静态 / prim 静态 / 实例方法全部路径。
- 🟢 T4 实例泛型方法（Option.Map）核实：走 `_bindInstanceMemberCall` 的 `_withDefaults`（:91/:151/:255）
  ⇒ 同一注入点已覆盖，无需单独接线。

## 测试

- 🟢 T5 运行期真门 `src/tests/generic-methods/lambda_inference.z42`：sumWith / applyTwice / fold
  无标注省略 `<>` 真跑断言（60/13/106），**interp + jit 双绿**。
- 🟢 T6 `z42c.semantics` typecheck 6 条单测：free/static 无标注能编 + 纯 A 标注驱动 where + 毒化解除 +
  where 满足对照 + 无标注纯 A 静默。**退回对照坐实**（见下 ⚠️）。
- 🟢 T7 删除 `PROBE_lambda.z42` 临时探针。

> ⚠️ 退回对照抓到**两个假门**（已修）：`pure_a_annotated` 与 `lifts_poisoning` 初版 lambda body 写了
> `x+1`——退回态 x 绑成裸 T、`x+1` 自己报 E0402 凑巧同数 ⇒ 测不出。改用**恒等/常量体**（`(int x)=>x`
> / `(x)=>1`），错误只来自推断驱动的 where 检查，退回态归 0，成真门。

## GREEN + 落地

- 🟢 T8 完整 GREEN：`xtask test` 全 stage 绿 + 自举不动点 3/3 gen1==gen2；`test stdlib --mode jit` ✔ 0 failed；
  `test bootstrap` ✅ nightly z42c 编当前源无越界；e2e `lambda_inference` interp+jit 双绿。
- 🟢 T9 文档同步：`generics.md` 加「lambda 实参驱动推断」节 + 更新推断/显式对比表；`generic-methods.md` 补引用。
- 🟢 T10 归档 changes→archive + 本 tasks 改 🟢；PR（合并前 rebase+重跑 GREEN）。
