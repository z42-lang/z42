# Tasks: tighten-implicit-conversions（PR2）

顺序要点：**先加基础设施 + 迁移源（旧行为下仍绿）→ 再翻门收紧**，避免 z42c 自编不过。

## T1 基础设施（不改变行为，可先落）
- [ ] `DiagnosticCodes.z42`：`ImplicitNarrowingConversion = "E0439"`。
- [ ] `TypeChecker.z42`：`ConvertIfNeeded(value, target)` + `_repClass` + `_isNumericPrim` 复用。
- [ ] `TypeChecker.z42`：`CheckImplicitConvert(value, target, syms, sp, ctx)` + `_isIntegerPrim` +
      `_constIntInRange(value, target)`（覆盖 BoundLitInt / 一元负号 / const 折叠字面量 + 整数范围表）。

## T2 接线 ConvertIfNeeded（仅插 ConvertInstr；检查/诊断留到 T4 与收紧耦合）
> 精化（2026-08-11）：`CheckImplicitConvert`（E0439 + 常量例外）只在收紧门后才有意义，故其接线**下移到 T4**，
> 与翻门耦合——避免宽松门下白白改动下转/拆箱的诊断码与错误文案。T2 只接 `ConvertIfNeeded`（修拓宽表示，
> 与门正交）。收紧门下窄化仍放行、但 repClass 同类不插 convert → 窄化点在 T4 前继续旧行为（`byte b=300`→300）。
- [ ] `ConvertIfNeeded` 加在每个 `BoxIfNeeded` 调用点之后（Box 与 Convert 互斥，组合安全）：
      StmtBinder return(156)/var-decl(185)、TypeChecker.BoxArgs(89)、OverloadBinder params-pack(83)、
      ExprTyper 调用实参(84/89/313)/数组元素(379/455)。
- [ ] 行为变化：`double d = 5` 等拓宽隐式现插 ConvertInstr（I64→F64，修表示 bug）；窄化/下转/拆箱**不变**。
- [ ] 产物字节变（拓宽点多 ConvertInstr + z42c 自身 codegen 变）→ 自举破一代，warm 重建收敛 5/5。
- [ ] golden：拓宽点字节变的 golden 需 regen（确认仅拓宽相关）。

## T3 迁移源（先迁移，后翻门）
- [ ] 临时翻门（本地实验）→ `xtask build stdlib` 抓真窄化点 → 逐点补 `(T)`。
- [ ] 核对：在范围常量点**不应**报（验证常量例外正确）；只有非常量/越界/有损浮点隐式点需 cast。
- [ ] z42c 源自身：自举 gen2 暴露的真窄化点补 cast。
- [ ] grind 至 stdlib + z42c 源在收紧门下全编过。

## T4 翻门收紧（正式）
- [ ] `Conversion.z42`：`ImplicitOkPermissive`→`ImplicitOk`，剔除 `ExplicitNumeric`。
- [ ] 全量重编：stdlib + z42c + toolchain 绿。

## T5 测试
- [ ] `tests/conversion/`：负向（非常量窄化 / long→int / float=intVar 拒绝）。
- [ ] `tests/conversion/`：常量例外（byte b=48 接受 / byte b=300 → E0439 / sbyte=-1 接受）。
- [ ] e2e：`(byte)300==44`、`byte b=48`→48、越界报错、`double d=5` 真 F64 运算。

## T6 文档 + 收尾
- [ ] `docs/book/src/compiler/type-conversion.md`：收紧门 + 常量例外 + ConvertInstr 插入机制 + 更新种类表。
- [ ] `xtask test` 全绿 + 自举 gen_n==gen_{n+1}。
- [ ] `xtask test bootstrap`：上一 nightly 编迁移后源无越界。
- [ ] commit（拆分：基础设施 / 迁移 / 收紧+测试 分逻辑单元）→ PR（User 手动合）。
- [ ] 归档 + 更新 memory 续推口令状态到 PR3。
