# Design: 编译期 IR 优化管线

> 对齐：2026-07-30。总准则见 book [optimization-pipeline](../../../book/src/runtime/optimization-pipeline.md)（准则 1 interp-first / 准则 2 内存时间开销）。

## Architecture

```
z42c.semantics: IrGen.Generate(cu, model)
    emit IrModule（朴素）
        │
    ▼ IrOptPipeline.Run(module)   ← 本 change 新增（compiler 源码,非 stdlib API）
        const-fold → copy-prop → temp-DCE   （单趟,只碰单赋值 temp）
        │
    ▼ 优化后 IrModule → ZbcWriter → zbc（更少指令）
        │
    interp（少 dispatch）/ JIT（少编 + Cranelift 二次优化）
```

## Decisions

### Decision 1：pass 放 compiler 源码，不放 stdlib z42.ir（避开 bootstrap axis ②/④）

**问题**：pass 要读指令的 Dst/操作数、重建 block。这些类型（`IrInstr`/`IrBlock`/`IrFunction`）在 stdlib `z42.ir`。
**约束**：`z42.ir` 是 z42c **运行期自依赖**的库（axis ④）;给它加 pass 调用的**新方法**（如 `IrInstr.WrittenReg()`）
= 新 stdlib API 面,z42c 源调用它要**晚一个 nightly**（axis ②:种子 z42.ir 还没这方法 → 冷启动 undefined）。
**选项**：
- A：pass 放 `z42.ir`（stdlib）+ 给 `IrInstr` 加 `WrittenReg`/`ReadRegs` virtual → 触发两-nightly 纪律,拖慢。
- B：pass 放 `z42c.semantics/src/IrOpt/`（**compiler 源码**）,**只用 z42.ir 现有 public 字段**（`IrBlock.Instrs`/
  `IrFunction.Blocks` 已 public;各 opcode `.Dst/.A/.B` 已 public）,在 pass 内 **type-switch** 每个 opcode 读写。
**决定**：**选 B**。零 z42.ir API 变更 → 零 bootstrap nightly 延迟。代价:pass 内要枚举 opcode（type-switch,
类似 ZbcWriter `_regtInstr`）;**未知 opcode 一律保守**（假设读全部寄存器 + 有副作用 → 不优化、不跨越），安全。

### Decision 2：只碰单赋值 temp（保守正确性边界）

**依据**:z42c 表达式临时寄存器**单赋值**（每 temp 全新 `Alloc`）;命名局部**重赋值**（`Copy` 拷回）。
**决定**:三 pass 只在**单赋值 temp** 上动:
- copy-prop:`X = expr`(temp t) + `Copy(local, t)` 且 **t 全函数仅此一次被读** → `expr` 直接写 local,删 Copy。
- temp-DCE:单赋值 temp 的 Dst **零读** 且指令**无副作用**（Const/算术/比较等纯计算;Call/VCall/字段写/数组写不删）→ 删。
- const-fold:`ConstX t1` + `ConstX t2` + `Add d=t1,t2`（t1/t2 均单赋值 const,仅喂该算术）→ `Const d=<折叠值>`。
命名局部的 DCE/const-prop 需 def-use/liveness → **本 change 不做**（Out of Scope）。

### Decision 3：pass 单趟 vs 迭代到不动点

const-fold 产出新 const 可能解锁新 copy-prop/DCE。**首版单趟** `const-fold→copy-prop→temp-DCE`,量收益;
若剩余冗余明显再上「迭代到不动点」（worklist）。遵准则 2:pass 开销与收益匹配,不过早上重机制。

### Decision 4（关键权衡，需 User 裁决）：甲 编译期 vs 乙 VM 加载期

实施探查中浮现一个**之前未摆到台面的成本**,按事实校正责任必须让 User 知情后再定:

| | 甲：z42c 编译期（本 change 当前假设） | 乙：VM 加载期（loader 后处理 IR） |
|---|---|---|
| interp 收益 | 相同（emit 更少指令） | **相同**（interp 跑的是同一份优化后 IR） |
| bootstrap 成本 | **每次优化变更都改 z42c 自身产物** → 自举 `gen1==gen2` 需重稳定（warm 重建两遍才收敛;CI 冷启动种子无 pass → 走两代自举路径） | **零**（VM-local,不碰 z42c 产物,golden zbc 不变,自举不受影响） |
| 运行时代价 | 零（烘进 zbc） | 每次加载跑一遍 pass（可缓存;对长跑程序可忽略） |
| 架构正统 | 正（优化属编译器） | 次（执行器做了本属编译器的事） |

**self-host fixpoint 机制**（已核实 `scripts/build/xtask_compiler.z42`）:gen1 = 当前 in-tree z42c 自建;
gen2 = gen1 再建;查 gen1==gen2。优化 pass 改 z42c 自身 bytecode → 引入当次 gen1（旧 warm driver 产,未优化）
≠ gen2（gen1 产,已优化）,需 warm driver 先稳定成 pass-having 再快照。**这是 pre-1.0 每个编译期优化 change
都要付的税**。

**Claude 建议**:鉴于**准则 1（interp-first）下甲乙 interp 收益完全相同**,而乙**零 bootstrap 税**、可先快速
验证收益、且**日后证明有效再下沉 z42c**（甲）—— 曾推荐乙起步。

**User 裁决（2026-07-30）：选甲（编译期）**。区分清楚——**能在编译期做的就编译期做,减少运行时开销**。
理由与**准则 2** 一致:编译期算掉 = 运行时零 pass 时间/内存开销;自举税是 pre-1.0 该付的成本,不用它换
运行时的持续开销。→ 本 change 按甲实施:pass 挂 `IrGen.Generate` 末尾,产物即优化后 zbc。

## Implementation Notes
- pass 入口 `IrOptPipeline.Run(IrModule) -> IrModule`（或原地改 `Blocks`/`Instrs` 数组）。
- **⚠️ 正确性不变量：参数寄存器 live-out（实施期发现，out_var 回归）**。DCE「Dst 全函数零读 → 删」
  遗漏了一类 escape：**out/ref 参数的最终值由调用方读取**（`v = 42` 写 out 参数 `v`，函数内不再读 `v`）——
  函数内 read-count 看不到跨函数的这个读 → 误判死码删掉写 out 的指令 → 调用方读到 Null。
  修复：DCE 前把参数寄存器种为 live（`reads[0..ParamCount] += 1`，上界含一格覆盖 this/env）。
  **一个寄存器的值 escape 函数的全部途径**：① 返回（RetTerm 读，已计）② out/ref 参数最终值（本修复）
  ③ 有副作用指令读（已计）。三者齐全 DCE 才安全。
- 「t 仅此一次被读」:一趟扫全函数统计每个 temp reg 的读次数（type-switch 各 opcode 的读操作数）。
- 重建 block:new `IrBlock(label, newInstrs, newCount)`（构造器已 public）。
- **不碰 MaxReg 下调**（删指令后 reg 号有洞无妨;REGT 按 MaxReg 仍正确;下调是额外优化,留后）。

## Testing Strategy
- 单元（pass 级,`IrOpt/*_tests.z42`）:构造小 IrModule → Run → 断言指令数下降 + 语义等价（关键 Dst 值不变）。
- golden/e2e:`xtask test e2e`（interp）+ `--mode jit` 全绿,输出不变（优化保语义）。
- **自举**:`xtask test compiler`（gen1==gen2,若走甲需先 warm 稳定）+ golden regen。
- **interp 前后基准**（收益量化,先测再投）:选 array-loop/poly/fib 等,量优化前后 interp 时间 + 指令数降幅。
- 若走乙:上述自举/golden 不受影响,只需 e2e 保语义 + interp 基准。

## Deferred / Future Work
- 命名局部 DCE/const-prop（需 def-use）;CSE;MaxReg 下调重编号;迭代到不动点;指令合并/专用指令。
- 运行时 JIT/interp 分层 + 旧指令内存回收（准则 2 运行时面）→ change `runtime-jit-tiering`。
