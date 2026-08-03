# Tasks: CSE 公共子表达式消除（新 Opt.Cse 位）

> 状态：🟢 已完成 | 完成：2026-08-03 | 类型：perf（compiler，优化管线）| 创建：2026-08-03
> "全部做" 优化程序 2/4（1=cascade copy-prop 已合；后续 3=多块内联 / 4=LICM）。复用 cascade 的 ReplaceReads 基建。

**变更说明：** 新增 `Opt.Cse` 位（16，All→31）+ `IrOptPipeline._passCse`：**块内** value-number
纯计算 op（arith/cmp/位/一元/convert，dst 单赋值 + 操作数稳定）→ 同 key 复现记 dupDst→firstDst，
用 `IrOptInfo.ReplaceReads` 全函数改写 dupDst 使用点为 firstDst、删重复计算指令。接线：inline →
const-fold → **cse** → copy-prop → dce（const-fold 后跑，先折常量再去重）。
**原因：** 消除块内重复纯计算（如 `(a+b)*(a+b)` 只算一次 add），interp 少 dispatch。
**文档影响：** book 优化页（新 pass 节）；README（IrOptInfo CseKey/DstReg + IrOptPipeline _passCse）。

## 独立性（D2）
- 新 `Opt.Cse` 位；CSE 单独开正确（块内检测 firstDst 支配 dupDst 及其使用点 → 值恒等；不依赖其它 pass）。

## 安全边界
- **块内**检测（同块 firstDst 在前 → 支配）；跨块不做（避支配分析）。
- 操作数须**稳定**（单赋值 temp `defs==1` / 从不重写形参 `defs==0`）→ 两次出现同值；dst 须单赋值（remap 有效）。
- Div/Rem 安全：首个在前，trap 则控制流不到第二个，否则同值。含分配/副作用/可空解引用 op 不入（StrConcat/FieldGet/Call/…）。

- [x] 1.1 `OptSet`：`Cse=16` / `All=31` / `ByName("cse")`
- [x] 1.2 `IrOptInfo.CseKey`（op|操作数ids，含 dst/操作数资格）+ `DstReg`（CSE-able op 的 dst）
- [x] 1.3 `IrOptPipeline._passCse`（块内 value-number StrMap + ReplaceReads 改写 + 删 dup）+ 接线
- [x] 1.4 单测：`(a+b)*(a+b)` CSE 复用（-O0 两 add / Cse 一 add）
- [x] 1.5 `xtask test` 全绿 + self-host 不动点（引入当次 gen1≠gen2 破一代=D7，重建自愈）
- [x] 1.6 文档同步（book + README）
