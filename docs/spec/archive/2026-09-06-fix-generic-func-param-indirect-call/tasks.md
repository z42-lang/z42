# Tasks: fix-generic-func-param-indirect-call

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

**变更说明：** `where T : Func<...>` 约束下把型参形参当函数调用（`f(x)`），binder 现在绑成
**间接调用**。此前绑成自由函数调用并报 E0401，只靠 emitter 的名字查表侥幸补救——**名字一旦被
lambda 捕获就硬炸**。

**原因：** `MemberResolver` 的 `call.Callee is IdentExpr` 分支只认 `vfn is Z42FuncType`，
`Z42GenericParamType` 不匹配 → 落到自由函数查找 → `E0401: undefined function: f`。
而 `CallEmitter` 仅凭 `Locals.ContainsKey("f")` 就发 `CallIndirect`，于是**直接调用**能跑、
诊断没人看（`--emit-zbc` 吞掉）。同文件下方**早就有**一条 `Z42GenericParamType` 间接调用分支，
只服务 `arr[i](x)` / `m()(x)` 这类**非** IdentExpr callee ——不对称就只在 IdentExpr 这一支。

emitter 的补救是**纯名字查表**，名字不在当前帧 `Locals` 里就失效：

```z42
int ApplyViaLambda<T>(T f, int x) where T : Func<int, int> {
    Func<int, int> g = n => f(n);   // f 被捕获
    return g(x);
}
```
binder 把 `f(n)` 绑成 `BoundCall "free"` → 闭包捕获分析**根本没看见** `f` 是被捕获变量 →
不发 `mk_clos`，lambda 体里发 `call @f` 调一个不存在的自由函数 →
运行期 `undefined function: Demo.f`（实测）。

**文档影响：** `docs/book/src/language/generic-constraints.md` 限制 4 做事实校正；
`GenericConstraint.z42` 抬头注释同款校正。

## 1. 根因修复
- [x] 1.1 `MemberResolver.z42` 的 IdentExpr 调用分支：补 `vfn is Z42GenericParamType` 情形 →
      `BoundIndirectCall`（镜像同文件已有的非-IdentExpr 分支；返回类型取 Unknown——func-type
      约束本身仍是 Deferred，拿不到 R）

## 2. 测试
- [x] 2.1 `src/tests/generics/func_constraint_captured.z42`（新增）：直接调用（不回归）/
      lambda 捕获 / 嵌套两层 lambda / Action 形态
- [x] 2.2 既有 `func_constraint_{basic,literal,predicate,action}.z42` 四条不回归

## 3. 文档同步
- [x] 3.1 `docs/book/src/language/generic-constraints.md` 限制 4：删掉「代码生成依赖该约束把参数当
      func 值走间接调用」这句**错误**断言，改写为事实校正 + 回归守卫指路
- [x] 3.2 `GenericConstraint.z42` 抬头：同款事实校正（原注「注意 CallEmitter 靠该约束…改动需谨慎」）

## 4. 验证
- [x] 4.1 `xtask build compiler` 自建全绿
- [x] 4.2 最小复现从「运行期 undefined function」变为通过
- [x] 4.3 完整 `xtask test` GREEN（含 self-host 不动点）
- [x] 4.4 分支基于 origin/main 顶 → PR

## 备注
- **两处错误断言是同一句话的两个副本**（源码注释 + book），都写着「codegen 依赖该约束」。
  实际 `CallEmitter` **从不看约束**，只查 `Locals.ContainsKey`。这句话让人以为这条路已经接通，
  是这个 bug 长期没被追查的直接原因——与 E0410 的「非真错」记述、`static_abstract_operator.z42`
  抬头的「TryBindOperatorCall 已处理型参」同型。
- **自举安全**：z42c / stdlib 无 func-type 约束写法；纯 binder 侧改动、无格式 bump、无新语法。
- **本修复是 [[restore-emit-zbc-diagnostics-program]] 的第 4 步第 3 项**（口令「推进诊断可见性」），
  三条静默错误代码之三。之一 = `fix-autoprop-getonly-backing-write`（PR #502），
  之二 = `fix-generic-operator-constraint-dispatch`（PR #505）。
