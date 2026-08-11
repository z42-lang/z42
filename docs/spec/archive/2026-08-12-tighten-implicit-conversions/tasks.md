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

## T3 迁移源（先迁移，后翻门）✅ **迁移面为零**
- [x] 翻门 + `build stdlib` grind：**stdlib 零窄化错误**——常量例外覆盖全部（Tar.z42 的 12 处在范围常量全合法）。
- [x] z42c 源自身：自举 gen2（`xtask test` 收紧门下）**逐字节等价**，无真窄化点。
- [x] 结论：PR2 未改一处 stdlib / z42c 源（唯一改动是 math 测试的 int-vs-double 松比较，属 T2 修 Pow 的连带）。

## T4 翻门收紧（正式）✅
- [x] `TypeFactsTc._isAssignable` → `Classify(...).ImplicitOk()`；`CheckImplicitConvert` 内门 → `ImplicitOk()`。
- [x] 3 个隐式上下文检查点（return / var-decl / assign）改用 `CheckImplicitConvert`（E0439 + 常量例外）。
- [x] 全量 `xtask test` 绿：241 unit + e2e + cross-zpkg + 自举 5 包 byte-identical。

## T5 测试 ✅（conversion 20→29）
- [x] 收紧门布尔投影 `test_strict_projection`（ExplicitNumeric 不放行）。
- [x] 负向：非常量窄化 / `long→int` / `float=intVar` → E0439。
- [x] 常量例外：`byte b=48` 接受 / `byte b=300` → E0439 / `sbyte s=-1` 接受。
- [x] 拓宽：`double d=5`/`long l=5` 合法 + `test_widening_emits_convert_node`（`(convert …)` 节点）。

## T6 文档 + 收尾
- [ ] `docs/book/src/compiler/type-conversion.md`：收紧门 + 常量例外 + ConvertInstr 插入机制 + 更新种类表。
- [ ] `xtask test` 全绿 + 自举 gen_n==gen_{n+1}。
- [ ] `xtask test bootstrap`：上一 nightly 编迁移后源无越界。
- [ ] commit（拆分：基础设施 / 迁移 / 收紧+测试 分逻辑单元）→ PR（User 手动合）。
- [ ] 归档 + 更新 memory 续推口令状态到 PR3。
