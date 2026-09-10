# Proposal: resolve-method-where-at-decl

> 状态：🟢 已实施 | 创建：2026-09-11 | 类型：fix（含新发诊断）

## 一句话

把方法级 `where` 的**声明级**诊断从「调用点即席重建 bundle 时顺带报」挪到**声明期**（Pass 0.5），
并删掉单文件路径上重复的那次 `Resolve`。关闭 Deferred `constraint-decl-diag-per-callsite`。

## 修前四个症状

| # | 症状 | 性质 |
|---|------|------|
| ① | 类级声明诊断在**单文件路径**报两遍 | 噪音 |
| ② | 方法级被调用 N 次 → 报 N 条 | 噪音 |
| ③ | **从不被调用的泛型方法，其 where 的声明级错误一条都不报** | 🔴 **真漏报** |
| ④ | `where U : IFoo`（`U` 不是该方法的型参）方法级**完全静默**（类级早就报了） | 🔴 **真漏报** |

③④ 才是这条 Deferred 被低估的部分——它原先被记成「纯噪音、无正确性后果」。实际上
`where T : IFooo` 把约束名拼错，等于**没写约束**，而在方法从未被调用时没有任何人告诉你。

## 根因

- ①：**两次 `Resolve`**。`IrDump.BuildModuleD` 显式调一次，紧接着 `Infer(cu, symbols)` 的
  `resolveConstraints` **默认就是 true**、进去第一件事又调一次。（`IrDump` 的另一条 dump 路径同款。）
- ②③④：`ConstraintChecker.Resolve` 只遍历 `ClassDecl` 的 where；方法级 where 的唯一入口是
  `CheckMethod`，而它只在**真被调用**时才跑，且每个调用点都从头 `_fillBundle` 一遍。

## 做法

1. `_fillBundle` 增 `bool report`：所有**声明级**诊断（E0443 / E0423 / E0402 class·struct 互斥 /
   E0453 绑定名笔误）都受它控制。
2. `Resolve` 除 `ClassDecl` 外，再走**类成员方法**与**顶层自由函数**（`MethodDecl.IsFree`）的
   where —— `_diagnoseMethodWheres`，report=true，**bundle 丢弃**；顺带补上 ④ 的 E0401
   （判据与类级 `_buildSet` 逐字同款）。
3. `CheckMethod` 照旧即席建 bundle，但 **report=false**。
4. 删掉 `IrDump` 里重复的那次 `Resolve`，让单文件与包并行两条路各有且只有一个。

**刻意不做缓存**：方法级约束没有一把稳定的键（同名同 arity 的重载各有各的 where），造一张键不可靠的
缓存表比即席重建（约束项个位数）危险得多。拆的是「报诊断」与「建 bundle」，不是加缓存。

## 明确不动

**违反约束**（`E0402` 实参不满足 / `E0422` 函数签名不符）仍**每个调用点各报一条**——那本来就是
per-call-site 的事实，同一个方法被调 3 次传 3 个不合格实参就该报 3 条。用例
`test_call_site_violation_still_reported_per_callsite` 钉住这条。

## Gate

见 [`tasks.md`](tasks.md)。全仓**零新增违反** ⇒ 新诊断必须自带 fixture，否则又是从不响的门。
