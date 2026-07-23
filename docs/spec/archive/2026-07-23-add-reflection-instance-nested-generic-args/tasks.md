# Tasks: 实例路径嵌套泛型反射（obj.GetType() nested generic args）

> 状态：🟢 已完成 | 创建：2026-07-23 | 完成：2026-07-23 | 分支：feat/reflection-instance-nested-generics（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；直接扩展刚合入的 add-reflection-nested-generic-args）

**变更说明：** `new Box<Pair<int,string>>().GetType().GetGenericArguments()` 的嵌套实参此前
`inner.IsGenericType==false` 且 `arg[1].Name==" string"`（带前导空格）——因 `ObjNew` 用
`Z42Type.Name()` 发实参（**短名 + `", "` 分隔**）：短名 `"Pair"` runtime 解析不到真句柄、
`", "` 的空格被 `split_generic_args` 带进 arg 名。

**原因：** typeof 已由 add-reflection-nested-generic-args 用 `_typeofArgName`（FQ + `","` +
递归）修好；实例路径（ObjNew）没同步，两条路径不一致。

**修复（一行，复用刚合入的 helper）：** `ExprEmitter._emitNew` 的 ObjNew 实参循环从
`inst.TypeArgs[k].Name()` 改为 `this._typeofArgName(inst.TypeArgs[k])`——FQ + 递归带尖括号，
runtime `make_type_from_name` 的括号解析（已合入 main）自然递归。**无 runtime 改动、无格式 bump**。

**文档影响：** `docs/design/language/reflection.md`（nested-generic-args Deferred「剩余」项标记
实例路径已闭环）。

- [x] 1.1 `ExprEmitter.z42`：ObjNew 实参改用 `_typeofArgName`
- [x] 1.2 `src/tests/types/instance_nested_generic_args.z42`：e2e（嵌套实例 == typeof / 平铺不回归）——interp+jit 空输出 exit0
- [x] 1.3 全绿验证：types e2e **72 pass 0 fail**（`default_generic_param*` + `instance_generic_args` 全绿，**无回归**）+ stdlib ✔ + **compiler 自举不动点 5/5 gen1==gen2 byte-identical**
- [x] 1.4 `docs/design/language/reflection.md`：nested-generic-args「剩余」项标记实例路径已闭环
- [x] 1.5 归档 + PR

## 备注
- 风险点：ObjNew.type_args（ScriptObject.type_args）也被 `default(T)`（DefaultOf 读 tag 判 prim/ref）
  与泛型 dispatch 消费。短名→FQ 不改 prim/ref 性（`int` 仍 `int`；用户类仍 ref），故 default(T)
  预期不受影响；泛型走 reified type-erasure（无 monomorphization），type_args 是元数据非 dispatch 键。
  以全绿套件为准验证。
- 基元实参（`new Box<int>()`）经 `_typeofArgName` 叶子路径 = `_typeofName(int)` = `"int"`，**不变**。
