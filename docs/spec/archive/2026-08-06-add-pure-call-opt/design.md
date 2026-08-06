# Design: 纯函数调用优化（自动推断 + CSE/LICM）

## Architecture

```
IrModule ──IrPureFunctionTable.Compute(m)──> PureTable(funcName→bool)   [模块不动点，内存]
                                                     │
IrOptPipeline.Run(m, optSet):
  pureTable = PureCall 开 ? Compute(m) : null
  逐函数 _optFunc(f, optSet, pureTable):
     LICM  ── _isHoistablePureCall(call, pureTable) → 循环不变纯调用外提 pre-header
     CSE   ── CseKey CallInstr 分支(callee 纯 + args 全稳定) → 同调用消重
                                                     │
                                          ZbcWriter 序列化时忽略纯度 → 普通 call 字节
```

## Decisions

### Decision 1: 自动推断，不引入 `pure` 标注（User 裁决）
**问题：** 优化器要知道 Call 的 callee 纯不纯。
**选项：** A — 模块不动点自动推断；B — `pure` 关键字标注 + 校验。
**决定：** **A**。覆盖最大（同模块所有函数自动获益）、零用户负担、与刚合并的 `IrEscapeSummary` 逐字同构
（同 `_findFunc`、同不动点骨架、同"无摘要保守"底线）。soundness 由 IR 保守推断保证，不依赖任何标注。
`pure` 关键字契约 Deferred（真需要文档/防回归时再加，且是跨包 pure 的载体）。

### Decision 2: 纯度定义 = IsPure ∪ 纯调用 ∪ readonly-fget ∧ no-throw
**决定：** 函数 f 纯 ⟺ f 每条指令满足以下之一，且无块以 `throw` 终结：
1. `IrOptInfo.IsPure(ins)`（现有白名单：Const*/算术/比较/位/Neg/Not/Convert/Copy/IsInstance/AsCast/Typeof）
2. `ins is CallInstr && pureTable.IsPure(call.Func)`（对纯函数的直接调用）
3. `ins is FieldGetInstr && fg.Readonly`（**复用 readonly change 的标志**——readonly 字段值构造后不变）

**为何纳入 readonly-fget**：readonly 字段永不变 → 同 receiver + 同 args 恒同结果，CSE/LICM 复用/外提安全
（哪怕两次调用之间隔着其它代码，readonly 也不会变）。这把可判纯的函数从"纯标量"扩到"读自身 readonly
配置 + 参数计算"，覆盖面大增。接收者必是 arg 派生（纯函数不 StaticGet/不分配 → 持有的对象只能来自参数）。

**为何 no-throw**：LICM 提到可能零迭代的 pre-header，会抛的"纯"函数提前执行 = 异常时机漂移。CSE/LICM
共用一个"纯含不抛"判据。`Div`/`Rem`（除零陷阱）不在 IsPure → 天然被排除（含 Div 的函数非纯，保守但正确）。

### Decision 3: 分配排除出纯度
**决定：** 含 `ObjNew`/`ArrayNew`/`ArrayNewLit`/`MkClos`/`StrConcat`/`ToStr` 的函数**不判纯**。
**理由：** 分配无外部副作用，但 **CSE 消重会改变对象身份**（两次 `new` 消成一次 → 本应不同的对象变同一个，
破坏 `==` 引用相等 / GC 语义）。`IrOptInfo.IsPure` 已同款排除。分配优化交 `StackAlloc`/`LoopAllocReuse`。

### Decision 4: 模块不动点方向 = 乐观全纯 → 单调降级
**决定：** `Compute(m)` init 每个有体函数登记 `pure=true`（乐观）；不动点反复扫，`_isFuncPure(m,f,table)`
用当前表判——发现任一非纯指令 / throw 终结 / 调非纯 callee → 降级 `pure=false`（只降不升）→ 无变化即收敛。
**与 escape 对称**（escape 乐观全 false 上升 / pure 乐观全 true 下降），单调保证终止。**递归纯函数**因乐观
初值自然收敛为纯（相互递归的纯函数簇一起判纯）。imported/无体 → 不登记 → Lookup 返回"未知" → 调用点保守非纯。

### Decision 5: v1 同模块（imported 保守非纯）
同 readonly/escape：`_findFunc` 找不到（跨包/无体）→ 该调用点使函数非纯。跨包 pure Deferred。

### Decision 6: 无格式 bump（PureTable 内存分析）
`IrPureFunctionTable` 是编译期内存结构，优化在 `ZbcWriter` 之前消费。序列化的 `call` 字节不变 → 不 bump。

## Implementation Notes

### IrPureFunctionTable.z42（NEW，模板 = IrEscapeSummary.z42）
```
public sealed class PureTable {           // funcName → bool（纯）；未登记 = 未知 → IsPure 返 false
    private StrMap _map;                   // funcName → "1"（纯）；不存 = 非纯/未知
    public bool IsPure(string name) { return this._map.ContainsKey(name); }
    public void SetPure(string name) { this._map.Put(name, "1"); }
    public void Clear(string name)   { ... 需 StrMap 无 Remove → 用第二个 impure set 或重建 }
}
```
⚠️ StrMap 无 Remove（readonly change 已踩）——不动点降级不能"删 key"。方案：用 **bool[] 按函数下标**
（funcName→index 建一次映射），或 `PureTable` 内维护一个"已判非纯" set 单调加入。**采用 bool[] 按 m.Functions
下标**（最简、单调翻位，同 escape 的 `bool[] esc` 思路）：`Compute` 返回 `PureTable` 包 `bool[] pure`
（下标对齐 `m.Functions[i]`）+ name→index 的 StrMap（供调用点 `IsPure(func)` O(1) 查）。

`Compute(m)`：
1. 建 name→index（StrMap），`bool[] pure`，有体函数 `pure[i]=true`（乐观），无体 `pure[i]=false`（保守）。
2. 不动点：`changed=true; while(changed){ changed=false; for each 有体 f_i with pure[i]==true:
   if(!_isFuncPure(m, f_i, table)){ pure[i]=false; changed=true; } }`
3. `_isFuncPure(m,f,table)`：遍历 f 所有块——每块终结子非 `ThrowTerm`；每条指令 `IrOptInfo.IsPure(ins)`
   或（`ins is FieldGetInstr && fg.Readonly`）或（`ins is CallInstr` 且 `table.IsPure(call.Func)`）；否则返 false。

### OptSet.z42
`PureCall = 512`（bit9）；`All = 1023`；`ByName` 加 `"pure-call"`（readonly-load 之后）。

### IrOptInfo.z42
- `CseKey(ins, defs, paramCount, optSet)` → 加 `PureTable pureTable` 参数。CallInstr 分支：
  `if (ins is CallInstr) { CallInstr c = ins as CallInstr; if (Opt.Has(optSet, Opt.PureCall) && pureTable != null
   && pureTable.IsPure(c.Func) && _dstOk(c.Dst, defs) && 所有 c.Args[0..ArgCount) _stableR) return "call|"+c.Func+"|"+argIds; return null; }`
- `DstReg` 加 `if (ins is CallInstr) return (ins as CallInstr).Dst;`（与 readonly 的 FieldGet 同）。
- **无需失效表**：纯调用不依赖可变状态，块内同 key 恒同值（比 readonly 简单——readonly 需 FieldSet 失效）。

### IrOptPipeline.z42
- `Run(m, optSet)`：`PureTable pureTable = Opt.Has(optSet, Opt.PureCall) ? IrPureFunctionTable.Compute(m) : null;`
  传入 `_optFunc(f, optSet, pureTable)`。
- `_optFunc`：licm 门控 `Licm||ReadonlyLoad||PureCall`，传 `pureTable` 给 `IrLicm.Run(f, optSet, pureTable)`；
  cse 门控同加 `PureCall`，`_passCse(f, optSet, pureTable)` → CseKey 传 pureTable。

### IrLicm.z42
`Run(f, optSet, pureTable)`；`_hoistInvariants` 加 `_isHoistablePureCall(ins, optSet, pureTable, defInLoop, rc)`：
`Opt.Has(optSet, Opt.PureCall) && ins is CallInstr && pureTable != null && pureTable.IsPure(c.Func)
 && _operandsExternal(ins, defInLoop, rc)`（`_operandsExternal` 已通用覆盖 call args 全循环不变）。
与 `_isHoistableReadonlyFget` 并列 `||`。dst 单赋值仍由 `defsGlobal[d]==1` 守（纯 call dst 是单赋值 temp）。

### IrDump.z42
默认 dump 路径 optSet 现为 `Opt.All - Inline - StackAlloc - LoopAllocReuse`（防非确定/golden 漂移）——
**追加减 `PureCall`**，让既有 codegen golden 不因 pure-call 消重漂移；pure-call 单测显式传 `Opt.PureCall`。

## Testing Strategy
- codegen 单测（`codegen_tests.z42`，仿 readonly 的 `Opt.ReadonlyLoad`）：纯调用两次 CSE 消重 / 非纯不消重 /
  循环纯调用 LICM 外提 / 循环变参不提。
- 运行时 golden `src/tests/optimization/pure_call_hoist/`：开/关输出逐字节一致（正确性主门）。
- bench `pure_call_bench.z42`：热循环反复调纯函数，A/B（`--opt -pure-call`）记 PR。
- **GREEN 重点**：自举 gen1==gen2（z42c/stdlib 有纯函数 → 输出变，D7 一次性、重建自愈）+ golden 全绿。

## Deferred（登记 roadmap Deferred Backlog Index）
- **pure-future-cross-zpkg**：imported 函数纯度（读 `IrFunction.Attrs` 已序列化 / 或跨包摘要）。
- **pure-future-devirt-vcall**：去虚化后 VCall 判纯（final 类 / 单态化）。
- **pure-future-alloc-scalar**：含分配的确定性构造函数的标量替换（需身份分析）。
- **pure-future-relax-div**：可证非零除数时放宽 Div/Rem。
