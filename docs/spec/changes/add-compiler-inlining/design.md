# Design: 编译期函数内联 + 可独立开关的优化集（OptSet）

## Architecture

```
CLI --opt inline,dce / --no-opt copy-prop ─┐
project.toml [optimize] inline=true …       ├─► resolveOptSet ─► OptSet(位集) ─► req
profile(debug=None / release=All)           ┘        (CLI > toml > profile 默认)
                                                              │
                                        PackageCompile ─► IrGen.Generate(cu, model, optSet)
                                                                      │
                                              IrOptPipeline.Run(irm, optSet)
                                                 逐 pass:if Has(optSet, X) then run X
                                                   Inline? → const-fold? → copy-prop? → dce? → algebraic?
                                                   (固定稳妥顺序,只跑被勾选的;每个 pass 单独正确)
```

## Decisions

### D1: OptSet = 具名优化位集(取代数字档位)
z42 受限写法(static class + int 常量,同 TokenKind 模式)。每个优化一个位:
```
public static class Opt {
    public const int None      = 0;
    public const int ConstFold = 1;    // bit0（常量折叠 + 代数恒等式,同属 TryConstFold）
    public const int CopyProp  = 2;    // bit1
    public const int Dce       = 4;    // bit2
    public const int Inline    = 8;    // bit3
    public const int All       = 15;
}
// Has(set, opt) => (set & opt) != 0
```
OptSet 是一个 `int` 位集。**用户自助勾选任意子集**,不是"高档含低档"的单调关系。
> 代数恒等式在 `IrOptInfo.TryConstFold` 内,与常量折叠同属 `ConstFold` 一个开关(v1 4 个开关:
> const-fold/copy-prop/dce/inline);要更细粒度未来拆 TryConstFold 再加一位。

### D2: 独立性约束(对所有优化 pass,硬性)
**每个 pass 单独开启都必须正确**——不得把「另一 pass 先跑过」当正确性前提。
- ✅ 允许**增效依赖**:如 `Inline` 后 `Dce` 能删更多死代码;但只开 `Dce`(不开 Inline)也**正确**(只是删得少)。
- ❌ 禁止**正确性依赖**:任何 pass 都不能假设某寄存器已被 copy-prop 消除、某常量已被 fold 才不出错。
- **落地检查**:每个 pass 的单测**单独开启该 pass**(其余全关)跑 golden,验证结果正确。这条是 CI 级
  约束,防止未来新 pass 偷偷耦合。
- **顺序**:`IrOptPipeline` 内部按固定稳妥顺序跑**被勾选的** pass(Inline 靠前产生更多机会,清理类
  const-fold/copy-prop/dce 靠后)。顺序只影响**效果**,不影响任一子集的**正确性**。

### D3: 配置解析优先级
`CLI` > `toml [optimize]` > `profile 默认`(debug=None / release=All)。
- **toml**:`[optimize]` 段逐项 bool(`inline=true` / `const-fold=false` …);缺省项 → 取 profile 默认位。
- **CLI**:`--opt <csv>`(在 profile 默认基础上**加**这些位;`--opt all` = 全开)、
  `--no-opt <csv>`(**减**这些位;`--no-opt all` = 全关)。二者可组合,先加后减。
- 未知优化名 → 报错退出(不静默忽略)。

> **debug 默认 None**(User 裁决 B):调试构建忠实可调试是原则。用户仍可显式 `--opt inline` 在 debug
> 上按需开某优化(如复现 release-only bug),这正是「可独立开关」的价值。

### D4: 内联资格(保守 v1)
`dst = Call callee(args)` 内联当且仅当:① 直接调用(非 `VCall`);② callee 同模块可解析;
③ 非递归(callee≠caller 且不在内联栈);④ `callee.instrCount ≤ INLINE_MAX_SIZE(24)` **或**全模块
单调用点(恒内联);⑤ callee 无异常表 / 无 ref·out 参数 / 非闭包。命中阻断 → 跳过。

### D5: 内联展开机制
① 寄存器重命名(callee regs +offset → 新 caller regs)+ **reg_types 同步扩**(否则内联后 typed
CmpBr / JIT i64 特化失效);② 形参 ← arg 寄存器;③ 块拼接:单块单 Ret → 直接内联、`Ret r`→`dst=r`;
多块 → 重贴标签插入、每 `Ret`→`dst=r`+jump 续延块;④ `Call` 被展开体取代;⑤ **稳定序**处理调用点
(按 block/instr idx)→ 输出确定(自举不动点前提);⑥ per-caller 预算 + 内联深度上限(防爆炸)。

### D6: 调试信息
内联指令 line table 映射到 **callee 源行**(best-effort);release 本就 strip-symbols 进 `.zsym`。
完整 inline-frame 链 deferred。

### D7: 自举字节不动点(关键)
z42c 自建是 release → 内联作用在 z42c 编自己上。不动点 `gen1==gen2`(同 driver 编 workspace 两遍):
- **稳态**:canonical z42c 已含内联 → gen1、gen2 同规则产出 → 字节相同,不动点成立(靠 D5.⑤ 确定性)。
- **引入当次**:种子(无内联)编当前源 → gen1 未内联,gen2(gen1 编)已内联 → gen1≠gen2 破一代,
  gen2==gen3 自愈。self-host byte-identical 是 **opt-in soak,不在默认 GREEN gate**,且有 pair-gen
  兜底 → 不阻塞发布链。
- **纪律**:内联纯优化、**不新增语法/不改 zbc·zpkg 格式** → 不触发 bootstrap-seed 两阶段;旧 z42c 照
  编当前源(编出未内联),下一代补上。

## Implementation Notes
- `INLINE_MAX_SIZE = 24`;单调用点忽略阈值。
- reg_types 内联时接在 caller 后面(保 typed/JIT 特化)。
- 内联后块/branch_targets/fused_tails 由 runtime 加载期重算,z42c 只需产出正确块+标签+指令。

## Testing Strategy
- **独立性单测(D2 硬约束)**:每个 pass 单独开(其余关)跑 golden → 正确;`Inline` 单独开也逐字节一致。
- 内联单测:小函数/单调用点/递归拒绝/VCall·跨包·异常表·ref-out 跳过/reg_types 保留。
- 配置单测:CLI/toml/profile 优先级、`--opt`/`--no-opt` 加减、未知名报错。
- Golden(interp+jit):任意 OptSet 子集下执行逐字节一致(优化不改可观察语义)。
- 自举:`xtask test compiler`;内联引入 soak 按 D7 收敛。
