# tasks: fix-inline-breaks-ref-params

> 类型：**fix**（release 下 `ref` 形参被内联即崩）+ **test**（`opt_all` 铺到特性类目）
> 创建：2026-09-27 ｜ 出身：结构审计 2026-09 批 B「给 2–3 个泛型 fixture 挂 `opt_all`」的铺开版

## Why

### 先做测量：`opt_all` 从 11 铺到 267

上一刀（#888）挂了 3 个泛型 fixture 就炸出一个 release 必崩的 bug。本刀把 `opt_all` 铺到
**19 个特性类目**（`structs` / `types` / `classes` / `inheritance` / `interfaces` / `generics` /
`generic-methods` / `generic-method-invoke` / `closures` / `delegates` / `refs` / `tuples` /
`pattern-matching` / `reflection` / `ctor-reflection` / `static-ctor` / `partial-types` /
`user-conversions` / `operators`），253 个空文件。

**当场炸出两个既存的 release-only 崩溃**，都在仓库自己的测试语料里、都是 debug 全绿：

| 元凶 pass | 症状 | 用例数 | 本刀 |
|---|---|---|---|
| **Inline** | `ref` 形参的 `Value::Ref` 流进算术 | 7（`refs/*` 全部）| ✅ 修 |
| **Devirt** | 去虚化到不存在的 `Std.Type.GetType` | 1（`types/value_type_object_methods`）| ❌ 不修，见「不做」 |

### 缺陷：内联把 `ref` 的协议拆了

`ref` 靠「callee **入口** copy-in / **出口** copy-out」实现（`exec_function_body` 把持
`Value::Ref` 的形参槽解引用成底层值，`run_ref_writebacks` 在每条退出路径写回 caller 的
lvalue）。**内联把 callee 帧整个去掉了** ⇒ 没有入口解引用、没有出口写回 ⇒ 裸 `Value::Ref`
直接流进 body 的算术。

复现源就是 `src/tests/refs/ref_local`（10 行）：

```console
$ z42c build project.z42.toml --release && z42vm dist/x.zpkg --mode interp
Error: type mismatch in arithmetic: Ref { idx: 0, frame_id: 1 } vs I64(1)
  # debug 通过；--release --no-opt inline 通过；-O0 --opt inline 复现 ⇒ 单变量锁定 Inline
```

**interp 与 jit 都崩** —— 不同于 #888 那条只崩一侧。

## What Changes

`IrInline.z42`：新增 `_addrRegsOf(caller)`（caller 里由任一取址指令定义的寄存器集）+
`_passesAddress(call, addrRegs)`，接进 `_eligibleCallee` 作第 ⑧ 条资格；两个 Phase 共用同一份集合。

**判据取调用点侧**，因为 **callee 的 IR 里根本看不出哪个形参是 `ref`**
（`escape-analysis.md` 记过同一条信息缺口）：`Param.IsRef` 只影响 **caller** 发不发取址指令。
于是不必给 IR 加 per-param 标志、**不动 zbc 格式**。

| 取址指令 | `ref` 指向 |
|---|---|
| `LoadLocalAddrInstr` | 局部变量 |
| `LoadElemAddrInstr` | **数组元素** |
| `LoadFieldAddrInstr` | **对象字段** |

⚠️ **三种必须全覆盖**：只判第一种时实测 **4/7** 转绿，`ref_array_elem` /
`ref_field_array_elem` / `ref_obj_field` 照旧崩（本仓反复出现的「只做一格漏掉常见形态」）。

⚠️ **既有的「被写形参材料化」救不了它**：`_writtenParamsAll` 给被写形参 emit
`copy (p+offset), arg[p]`，而那个 arg 装的就是 `Value::Ref` —— 材料化出来的是**一份地址的
副本**，不是被指向的值。它解决的是「别把写踩到调用方实参寄存器上」，与解引用无关。

### 🔴 取舍（这一条本该由 User 裁决，我按本仓既有姿态先选了保守那条）

选**保守方向**：带 `ref` 形参的函数从此不在传地址的调用点被内联。

- **代价**：少一次优化机会。z42c 自己有 40 处 `ref` 调用点受影响。**代价未量化** ——
  我没有旧 driver 的二进制做 A/B，而「重建一个旧 driver 专为测这个」的成本超过收益。
- **另一条路**：让内联器**合成** deref + writeback。正确性更完整，但工作量大得多，且大概要给
  IR 加 per-param `ref` 标志（**动格式**）。
- **选择理由**：与 `escape-analysis.md` 的「铁律」同一姿态 —— **宁可少一次机会，不要错**。
  若将来 `ref` 小函数的内联成为实测热点，再做合成那条。

## 指纹：37 → 38

**bump 的理由是同一份源码的发码变**：带 `ref` 实参调用的源文件此前**编得过、只是运行期崩**，
现在那些调用点不再被内联 ⇒ 哈希一字未变而发码变，不 bump 就会命中旧条目、修复永不生效。

⚠️ 与近几档不同，**这一档 CI 的 fingerprint 守门看得见**（z42c 自己 40 处 `ref` 调用 ⇒
自举产物真的会变）。取号按当时的 main 现查：main 上是 37 ⇒ 本刀 38。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/Optimization/IrInline.z42`
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（指纹 37 → 38）
- `src/tests/**/*.opt_all` + `src/tests/**/opt_all`（252 个新 sidecar）
- `docs/internals/src/runtime/escape-analysis.md`、`docs/internals/src/devinfra/testing.md`

## Tasks

- [x] `opt_all` 铺到 19 个特性类目（253 个）→ 测量出 8 个红
- [x] 单变量隔离两簇元凶（`--no-opt inline` / `--no-opt devirt` 逐个试）
- [x] Inline 修复（三种取址指令全覆盖）
- [x] 7/7 个 `refs` fixture 在 `--opt-all` 下 interp + jit 全绿
- [x] 真工程 `--release` + interp/jit 从崩溃变成正确输出
- [x] **自举不动点 3/3 gen1==gen2**（逐字节）+ `test compiler` 零失败
- [x] `xtask test e2e` **740 passed, 0 failed**（含 252 个新 sidecar 的正面验证）
- [x] 入库字节基线（`zbc-format` / `zpkg-format`）**零漂移**
- [x] `CompilerFingerprint` 37 → 38
- [ ] GREEN：CI 全矩阵绿

## 阴性对照

分两级，都做过：

1. **撤回整条修复**（= 测量态）：7 个 `refs` fixture 全红，报
   `type mismatch in arithmetic: Ref { … } vs I64(1)`。
2. **只覆盖 `LoadLocalAddrInstr`**（半修）：4/7 转绿，指向数组元素 / 对象字段的 3 个仍红
   ⇒ 坐实「三种取址指令都是载荷，不是照抄」。

## 不做（Out of Scope）

- **不修 Devirt 那条**（`types/value_type_object_methods`：去虚化到不存在的
  `Std.Type.GetType`）。根因还没查，且与本刀的 pass 无关 —— 一个 PR 一件事。
  ⚠️ 因此 `value_type_object_methods` 是**唯一刻意不挂 `opt_all`** 的用例；
  **修好 Devirt 时连它的 sidecar 一起加上**，已在 `testing.md` 写明。
- **不让内联器合成 deref + writeback**（见上「取舍」）。
- **不铺 `cross-zpkg` / `perf` / `zbc-format` / `zpkg-format` / `osr` 等类目**：前两者走不同
  驱动/计时，后两者是入库字节基线（挂上必然改基线），`osr` 有自己的模式门控。
