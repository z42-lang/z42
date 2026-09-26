# tasks: fix-stackobj-inline-struct-leaf

> 类型：**fix**（release + interp 下必崩）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「批 B」的「给 2–3 个泛型 fixture 挂 `opt_all`」——挂上当场炸出的。

## Why

### 缺陷

**一个带 struct 字段的普通类**，`z42c build --release` 之后 `--mode interp` 运行**必崩**：

```
Error: StructFieldSetPrim base: expected a struct value (StructRef), got StackObject { idx: 0, frame_id: 1 }
  at Demo.NBox.NBox (line 7, col 50)      ← ctor 的 `this.Item = t`
```

12 行可复现（`struct P2 { int X; long Y; }` + `class NBox { P2 Item; }`）。

根因：`NBox` 不逃逸 ⇒ 逃逸分析把它分配进**栈 arena** ⇒ 内联 struct 字段的叶子读写
（`StructFieldGetPrim` / `StructFieldSetPrim`）拿到的 base 是 `Value::StackObject`，
而 interp 的两个处理器**各有一条 `Value::Object` 臂、都没有 `StackObject` 臂**
⇒ 落到 `as_struct_ref` 兜底 bail。

### 🔴 不是泛型问题

最初是给 `generic_inst_value_semantics` 挂 `opt_all` 后炸出来的，很容易记成泛型 bug。
**换成非泛型的 `NBox` 症状一字不差** —— 是 StackAlloc × 内联 struct 字段的通病。
判据 fixture 因此刻意用**非泛型**的最小形态。

### 为什么活了这么久：三层遮挡刚好叠满

| 遮挡 | 效果 |
|---|---|
| debug profile 不开优化 | 没有栈对象 ⇒ 走不到 |
| 全部 golden 走默认 emit-zbc 优化集（StackAlloc 关）| `opt_all` 当时 11 个，**10 个在 `optimization/`、1 个在 `closures/`** ⇒ `types/`·`generics/`·`classes/` 等特性类目**一个都没有**；而优化类目的形状是为触发 pass 挑的，不含「带 struct 字段的普通类」 |
| **JIT 侧一直是对的** | jit 泳道两边都绿 |

「两个后端只有一个错」是现有门禁最难发现的形状。而 **release + interp 是真实部署形态**：
`--release` 默认 `Opt.All`，没有 jit 的目标（wasm / 部分移动端）就跑 interp。

### 这是同一条不变量的第三例

`escape-analysis.md` 已经记了两次「新增/修改任何对象表示，必须同步每一处」：
根扫描的两半（`fix-stackalloc-misses-inlined-refs`）、JIT/OSR 的镜像
（`fix-jit-osr-stackobject`）。本条是第三例，已按这个框架写进同一页。

## What Changes

| 处 | 改动 |
|---|---|
| `interp/exec_struct.rs` | `struct_field_get_val` / `struct_field_set_val` 各加一条 `Value::StackObject` 臂，镜像 `Value::Object` 臂，经 `ctx.stack_arena.with_obj{,_mut}` 解析 |
| `interp/exec_struct_tests.rs` | 单测 `stack_object_inline_struct_field_roundtrips` |
| `src/tests/optimization/stackalloc_inline_struct_field/` | 新 e2e fixture（带 `opt_all`）|
| `src/tests/types/*.opt_all` ×3 | 给 3 个泛型 fixture 挂 `opt_all`（就是它把 bug 炸出来的那一刀）|

### 写屏障的不对称是刻意的

堆那条臂写引用叶子要过 `write_barrier_field`；**栈这条刻意不要** —— 栈对象不是堆槽，
其堆引用字段由**每轮根扫描 arena** 保活。这与 `exec_object.rs::field_set` 的既有口径一致，
注释里写明了，别「为了对称」补一个屏障（那会去 card-mark 一个非堆地址）。

## Scope（允许改动的文件）

- `src/runtime/src/interp/exec_struct.rs`
- `src/runtime/src/interp/exec_struct_tests.rs`
- `src/tests/optimization/stackalloc_inline_struct_field/{source.z42,opt_all}`
- `src/tests/types/{generic_struct_inst_field,generic_inst_value_semantics,generic_struct_chain}.opt_all`
- `docs/internals/src/runtime/escape-analysis.md`、`docs/internals/src/devinfra/testing.md`

## Tasks

- [x] 两条 `StackObject` 臂（get + set，各含 prim 叶子与引用叶子两个分支）
- [x] 实测六种形态 × interp/jit 全过（prim 读/写、**引用叶子**读/写、copy-out、值语义传参）
- [x] 真工程复验：`z42c build --release` + `--mode interp` 从崩溃变成正确输出
- [x] **证明两个 ref 分支是活码**：逐个插 `bail!` 确认被触达（`NEGCTL-GET-REF-REACHED` /
      `NEGCTL-SET-REF-REACHED`），不是写了不跑的代码
- [x] 阴性对照：撤回两条臂 ⇒ 新 fixture 与新单测**都变红**（且 jit 仍绿，坐实后端不对称）
- [x] `xtask test e2e` 全绿（740 passed, 0 failed；含新 fixture 双模式）
- [x] `cargo test --locked --workspace --lib`（**无过滤**）：1368 passed, 0 failed
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不改 JIT**。它本来就对；本条是把 interp 补齐到 JIT 的行为，不是两边一起改。
- **不给其余 fixture 批量挂 `opt_all`**。本刀只挂 3 个泛型 fixture（审计点名的那几个）+ 新建 1 个。
  批量挂是可观的一刀（可能再炸出别的 release-only 缺陷），值得单独做、单独定性。
- **不动 `struct_copy_val` / `unbox_struct`**：它们的 base 是 arena `StructRef`，实测 copy-out
  与传参两格都已正确（走 `StructFieldGetPrim` 逐叶读出到新 arena blob），没有可达的 StackObject 形态。
  若将来有，按同一条不变量补臂。

## ⚠️ 自我更正：我把覆盖面数错了

第一版 PR / commit / 文档里我写「全仓当时只有 **2 个** `opt_all`」。**错的**：真实基线是 **11 个**。
我的 `find src/tests -name "opt_all"` 只匹配了 **dir 形态**，漏掉了全部 **flat 形态**
`<name>.opt_all`（8 个）。与我在 #882 栽的是同一个坑：**判据只认一种拼写**。

更正后的结论不但没被削弱，反而更准：缺口是**类目性**的 —— `opt_all` 只存在于
`optimization/`（+1 closures），特性类目整体为零。「10 个优化用例都挂了，却没有一个是
『带 struct 字段的普通类』」比「只有 2 个」更能解释这个 bug 为什么能活下来。
