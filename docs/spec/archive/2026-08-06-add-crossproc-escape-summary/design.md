# Design: 跨过程逃逸摘要

## Architecture

```
IrEscapeAnalysis.Run(m)
  ├─ IrEscapeSummary.Compute(m) → ParamEscapeTable        ← 新：模块不动点
  │     init paramEscapes[f][i] = false（乐观）
  │     repeat:
  │       for each func f: esc = _computeEscaped(m, f, table)   // 用当前摘要解析 call 实参
  │                        for each param i: if esc[reg(i)] → set table[f][i]=true (记 changed)
  │     until !changed                                     // 单调 → 收敛
  └─ 每函数 _markFunc(m, f, table)：用**收敛后**摘要重算 esc → 非逃逸+单赋值 ObjNew/ArrayNew/
        ArrayNewLit 置 StackAlloc（对象另需 ctor param0 不逃逸 = table[ctor][0]==false）
```

摘要**参数化** `_markEscaping`：`CallInstr`/`ObjNew` 的实参按 callee 摘要逐个判，其余调用保守全逃逸。

## Decisions

### D1: 参数逃逸摘要 = 逐函数逐参 bool 表，模块单调不动点

**问题**：摘要互相依赖（f 调 g，g 的摘要影响 f）+ 递归/相互递归。
**决定**：`ParamEscapeTable`（`bool[][]`，按函数索引 → 该函数各参数 bool；配 `nameToIdx` StrMap 供 callee 名解析）。
不动点：**乐观初始化全 false → 迭代把逃逸的置 true（只增不减，单调）→ 无变化即收敛**。单调保证收敛（最多
迭代 Σparam 次）。这是标准前向「may-reach-sink」最小不动点，正确。v1 全量迭代（每轮扫所有函数），慢再上 worklist。

### D2: `_markEscaping` 参数化 —— 实参按 callee 摘要，无摘要即保守

- **`CallInstr(callee, args)`**（静态直接调用）：`_findFunc(m, callee)` 得 callee 索引 → 逐 `args[i]`：
  `i < callee.ParamCount && !table[callee][i]` → **不标**；否则（摘要说逃逸 / 越界 varargs / callee 找不到=跨包）→ 标逃逸。
  - 实参↔形参对齐：static call `args[i]=param i`；ctor 经 CallInstr 时 `args[0]=this=param0` 也对齐（ExprEmitter base_ctor「args 含 this」）→ 统一 `args[i]→param i`。
- **`ObjNew(cls, ctor, args)`**：`args` 是 ctor 实参**不含 this**（运行时 obj_new 内部前置 this）→ `args[i]→ctor.param[i+1]`：
  `!table[ctor][i+1]` → 不标。ctor 找不到 → 全标。
- **`VCall`/`CallIndirect`/`Builtin`/`CallNative`/`MkClos`**：**保守全标逃逸**（动态派发/原生/闭包捕获，无可靠摘要）。
- **其余汇点不变**：FieldSet/StaticSet/ArraySet.Val、Ret/Throw、AsCast/ToStr/LoadLocalAddr/IsInstance/Convert —— 照旧标逃逸。

> **铁律不变**：仍完整镜像 `IrOptInfo.AddReads` 的读枚举——refine 只把「blanket 标 call 实参」换成「按摘要标」，
> 未被摘要覆盖的调用形式一律保守全标 → 绝不漏判逃逸（漏判 = 悬垂栈引用）。

### D3: `_ctorLeaksThis` 并入通用摘要

原 `_ctorLeaksThis(ctorName)`（判 ctor 是否泄漏 reg0=this 的单函数摘要）= `table[ctor][0]`。删除单独实现，
对象合格判据改查 `table`。统一：ctor 只是「param0=this」的普通函数，this-泄漏是 param0 逃逸的特例。

### D4: 摘要计算内部 —— 复用 `_computeEscaped`（吃摘要版）

`_computeEscaped(m, f, table)`：Pass A `_markEscaping`（吃 table）+ Pass B copy 闭包，**不变**，只是 Pass A
现按摘要标 call 实参。param i 逃逸 = `esc[reg(i)]`（reg(i)=第 i 个参数寄存器；实例方法 reg0=this）。
**返回参数天然算逃逸**：RetTerm.Reg 被标 + copy 闭包传导 → 返回的 param 其 reg 进 esc → 摘要置 true（保守正确）。

### D5: 无运行时改动（关键——为何安全）

C 只让**更多**对象标 `StackAlloc=true`，运行时**不变**：
- `StackObject` 早已跨帧——per-context arena，句柄自带 frame_id，任何帧经 `ctx.stack_arena` 按句柄 frame_id
  解析（**ctor 子帧访问 this 就是现成先例**）。对象传进 callee 帧 → callee 经 FieldGet 照常解析。
- 对象生命期系于**分配它的帧**；传进 callee、callee 返回后对象仍活（分配帧未退）。callee 若把它存到超出分配帧
  的地方 = **逃逸** → 摘要标之 → 不栈分配。故「非逃逸 ⟹ 不会活过分配帧」不变式**保持**。
- 触达面不变：非逃逸参数在 callee 内只会被读字段/写字段（存出去=逃逸已排除）→ 仍只经 FieldGet/FieldSet
  （+ ArrayGet/Set/Len for 数组），运行时已全覆盖。

### D6: 诊断 —— 沿用 escape 三防线 + 开/关对拍

摘要判**错**（把逃逸参数误判非逃逸）→ 逃逸对象被栈分配 → 帧退出后悬垂 = 内存损坏。防线：
1. **运行时 frame_id 悬垂校验**（add-escape）：栈句柄 idx 越界/frame_id 不符 → **明确报错、非静默 UB**——
   误判导致的悬垂访问会撞这道墙。
2. **逃逸汇点 debug 断言**（add-escape）：FieldSet/ArraySet/StaticSet.Val 存栈句柄 → 断言（反证摘要漏判）。
3. **开/关对拍（主门）**：`--no-opt stack-alloc` 关整条逃逸栈分配 → 与开启版**输出逐字节一致**（e2e golden）。
   摘要 miscompile 会表现为开/关输出不一致。
> 摘要本身**必须 sound over-approximate**（宁可多标逃逸、绝不漏标）——D2 的「无摘要即保守全标」是 soundness 底线。

## Implementation Notes

- **文件拆分**：摘要不动点独立 `IrEscapeSummary.z42`（`Compute(m)→ParamEscapeTable` + `ParamEscapeTable`
  持有者）；`IrEscapeAnalysis.z42` 消费它（避免超 300 行软限）。
- **reg(param i)**：参数寄存器 = 0..ParamCount-1（实例方法 reg0=this=param0）。与 `_seedParams` 口径一致。
- **nameToIdx**：`Compute` 建一次 StrMap（函数名→索引），callee 名解析 O(1)；跨包名不在表 → 保守。
- **`_computeEscaped` 签名**：加 `(m, table)` 参数；`_markEscaping` 加 `(m, table)`。摘要计算与最终标记共用同一函数。
- **只标单赋值 temp**（defs==1）不变（对齐现有口径）。

## Testing Strategy

- **单测**（`tests/escape-summary/`）：非逃逸参数（只读字段的辅助函数）摘要=false；逃逸参数（存字段/返回/传给
  逃逸 callee）=true；直接+相互递归收敛；跨包 callee 保守=true。
- **e2e golden**（`src/tests/optimization/escape_crossproc_*/`）：① `foo(new Point())` 循环里——对象经 C 栈分配
  （+ 若叠加 #118 则复用）；② 泄漏参数的 callee（`Store(new X())` 存全局）——对象**不**栈分配，输出正确、无悬垂。
  **开/关（`--no-opt stack-alloc`）输出逐字节一致**（主正确性门），interp+jit 双跑。
- **GREEN**：`xtask test` 全 stage，含 **self-host 5/5 gen1==gen2**（摘要作用于编译器自身、需字节不动点稳定）。
- **量测（实测 2026-08-06）**：循环内 `new Point` 传给非内联 `Consume`（8M）：
  - **C 单独**：7.545s vs 无逃逸栈分配 7.111s = **略负（~6% 慢）**——C 让对象栈分配但无 #118 复用 → arena 累积。
  - **C + #118（loop-alloc-reuse）**：5.013s(interp)/4.457s(jit) = **interp 1.42× / jit 1.59× 快**。
  - **关键结论**：此模式下 **C 单独 / #118 单独都不奏效**（无 C→对象经 call 逃逸→#118 的 C1 StackAlloc 失败→跳过；
    C 单独→栈分配但累积），**唯 C+#118 协同才赢**——C 让「传进 call 的对象」栈合格、#118 再 hoist+复用。
    C 的价值由 #118 解锁（#118 已合入 main，协同即时生效）。
