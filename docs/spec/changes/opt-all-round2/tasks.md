# tasks: opt-all-round2

> 类型：**test**（`opt_all` 覆盖收尾）+ **fix**（一个 fixture 的前提错了）
> 创建：2026-09-27 ｜ 出身：`fix-inline-breaks-ref-params`（#891）的第二轮铺开

## Why

#891 铺了 19 个类目（`opt_all` 11 → 268），炸出两个 release-only 崩溃（#891 / #892）。
本轮把余下 13 个类目铺完：`app-properties` / `attributes` / `basic` / `const` /
`control_flow` / `exceptions` / `gc` / `named-args` / `null_checks` / `params` /
`runtime-config` / `strings` / `symbol-resolution` —— 84 个用例，`opt_all` 268 → **352**
（32 个类目）。

**这轮只红 1 个**，而且是**断言失败不是崩溃** —— 说明前两轮把优化相关的崩溃基本清干净了。

## 那 1 个：`gc/gc_oom_exception` 的**前提**错了（不是编译器 bug）

```z42
try { var obj = new BigObj(42); } catch (OutOfMemoryException e) { caught = true; }
```

`obj` 从不被使用、压根不逃逸 ⇒ 全优化下**逃逸分析把它栈分配了**，一个字节都不碰 GC 堆
⇒ strict OOM 根本不触发、`caught` 恒 false。

| 构建 | interp |
|---|---|
| debug | ✓ |
| `--release` | ✗ |
| `--release --no-opt stack-alloc` | ✓ |

🔴 **这不是优化的 bug**：没人承诺「每个 `new` 都落 GC 堆」，不逃逸对象绕开 GC 正是栈分配的
**目的**。错的是用例的**前提** —— 它要测「strict OOM 拒掉一次**堆**分配且可 catch」，那就得让
那次分配真的是堆分配。

**改法**：在 try **之前**备好逃逸汇点（`BigObj[] sink`），try 里写 `sink[0] = new BigObj(42);`
（`ArraySet` 的 value 位是逃逸汇点）。sink 在设限之前分配 —— 设限之后建会先炸在 sink 上，
测的就不是 `BigObj` 了。

## 顺带核实并**排除**了一条我本来准备报的 bug

顺着「栈分配让分配对 GC 不可见」这条，我去查「**带终结器的类被栈分配，终结器还会跑吗**」——
这本会是一条真正的正确性缺陷。**实测不成立**：z42 **没有用户级终结器**（无 `~Class()` 语法、
参考手册无此概念），`FinalizerFn` 是宿主嵌入 API、不由 z42 用户代码为任意对象挂载。
⇒ 这条担忧不存在，**没有去编造一个 bug**。记在这里，免得下一个人重复这趟推理。

## Scope

- `src/tests/**`：84 个新 `opt_all` + `src/tests/gc/gc_oom_exception/source.z42`
- `docs/internals/src/devinfra/testing.md`

**不动编译器、不动指纹**（本轮零编译器改动）。

## Tasks

- [x] 13 个类目铺完 `opt_all`（84 个），总数 268 → 352
- [x] `gc_oom_exception` 改为强制堆分配
- [x] **阴性对照两条**：撤掉 `SetStrictOOM(true)` ⇒ 变红；`SetMaxHeapBytes(0)` ⇒ 变红
      （证明改后的用例**仍有判别力**，没变成恒绿的空门）
- [x] `xtask test e2e` 740 passed, 0 failed
- [ ] GREEN：CI 全矩阵绿

## 不做

- **不铺 `cross-zpkg` / `perf` / `zbc-format` / `zpkg-format` / `osr`**：前两者走不同驱动/计时，
  中间两者是入库字节基线（挂上必然改基线），`osr` 有自己的模式门控。
- **不碰 `gc_oom_exception` 的 `interp_only`**：jit 下 strict OOM 本就不生效（既有已知缺口，
  与本刀无关）—— 实测 jit 在 **debug 档也失败**，不是优化引入的。
