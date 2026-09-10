# Tasks: resolve-method-where-at-decl

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11 | 类型：fix（含新发诊断）

**变更说明：** 方法级 `where` 的声明级诊断挪到声明期 + 删掉重复的 `Resolve`。
关闭 Deferred `constraint-decl-diag-per-callsite`。详见 [`proposal.md`](proposal.md) /
[`evidence/baseline.md`](evidence/baseline.md)。

## 落地

- [x] `_fillBundle` 增 `report` 开关，四类声明级诊断（E0443 / E0423 / E0402 互斥 / E0453）受控
- [x] `Resolve` 增走**类成员方法** + **顶层自由函数**的 where（`_diagnoseMethodWheres`）
- [x] 补 ④：方法级 `where U : ...`（未知型参）报 E0401，判据与类级 `_buildSet` 同款
- [x] `CheckMethod` 改 report=false
- [x] 删 `IrDump` 里重复的 `Resolve`（`Infer` 的 `resolveConstraints` 默认 true 已经做了）
- [x] 单测门 `tests/typecheck/constraint_decl/`（9 条）
- [x] book `generic-constraints.md`：「已知限制 5」转 ✅ + 「校验发生在哪里」表改写
- [x] GREEN + 自举字节不动点

## 门的强度（两批退回对照，全部实测）

| 撤掉什么 | 预期红 | 实测 |
|---------|-------|------|
| 批 1：把 `IrDump` 那次重复 `Resolve` 加回来 + `CheckMethod` 改回 report=true | ①② | ✅ 正好这 2 条 |
| 批 2：摘掉两条声明期遍历（顶层自由函数 + 类成员方法） | ②③④ + E0423 那条 | ✅ 正好 5 条 |

> 批 2 里 ② 也红是对的：声明期遍历没了、调用点又 report=false ⇒ 该条从 1 变 0，
> 正说明它钉的是**声明期**那一条，不是「随便哪里报一条就行」。

## 关键数据

- **全仓零新增违反**：GREEN 13 stages 全绿 + 不动点 3/3，日志里一条新的约束诊断都没有
  （`grep E0443` 的 13 处命中全是测试名）。⇒ 新诊断在真实代码上恒为 0，**fixture 是它唯一的门**。
- **零字节漂移**：只动诊断的发出时机与次数，不动任何 bundle 内容 / 登记面 ⇒ 不动点 3/3 佐证。

## 附带订正

`generic-constraints.md`「已知限制 5」原把这条记成「纯噪音、无正确性后果」——**定性偏轻**。
真正值钱的是 ③④ 两个**漏报**：约束名拼错 / where 挂在不存在的型参上，在方法未被调用时
此前一声不吭。⭐ 又一次印证：**Deferred 里记的严重性也是「当时的推断」，接手前要自己复现一遍。**
