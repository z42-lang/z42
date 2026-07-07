# Tasks: 修 z42c 顶层同名自由函数导致 TSIG 导出崩溃

> 状态：🟢 已完成 | 创建：2026-07-08 | 类型：fix（compiler）
> 占用子系统：`compiler`（名义持有 = `add-file-level-incremental` DRAFT/未开工；
> User 指令"单独修复"，本 fix 与其零文件重叠——z42c.semantics vs z42c.project/pipeline，
> 借锁做小正交修复，归档时登记）

**变更说明：** 顶层（自由）函数以裸名注册进 `SymbolTable.Functions`（名唯一键——重载
mangling 仅类方法）。同名多定义静默覆盖 → `ExportedTypeExtractor._extractFunc` 用
`GetFunc(name)` 取到**错 arity** 的符号 → `md.Params[i]` / `sig.ParamTypes[i]` 越界 →
`FieldGet: not an object, got Null` 崩（编译器自身崩，非诊断）。
**原因：** redesign-xtask-test 实施期我在 xtask 写了同名不同 arity 的自由函数（`_enumerateCases`），
撞出此崩；当时用 distinct 名绕过，flag 为 follow-up。
**文档影响：** 无外部行为文档（新增一条编译错误码用法；诊断消息自解释）。

- [x] 1.1 `TypeChecker._checkDuplicateFreeFunctions`（Infer 内调用）：同 CU 顶层 func 同名 →
  `E0408 DuplicateDeclaration` 清晰诊断（"free functions do not overload"）
- [x] 1.2 `ExportedTypeExtractor._extractFunc` 崩溃加固：仅当 `ms.Signature.ParamCount ==
  md.ParamCount` 才信任解析签名，否则回落 decl 自身类型（有效唯一名代码路径不变 → 自举字节不变）
- [x] 1.3 复现验证：dup 自由函数 fixture → 新 z42c 报 `E0408 duplicate top-level function`（非崩）
- [x] 1.4 回归测试：`typecheck_tests.z42` +2（dup → 1 error；unique → 0 error，守 false-positive）
- [x] 1.5 GREEN：自举不动点 7/7 逐字节（守护）+ 新单测通过 + `xtask test` 全绿
- [x] 1.6 commit + push（compiler 锁下小正交 fix；`.claude`/`docs/spec` 纳入）

## 备注
- 自由函数**重载**是语言特性（需 spec 决策），本 fix 不加它——只把隐晦崩溃变成清晰"不支持"诊断。
  若日后要支持自由函数重载，另立 lang change（复用类方法的 arity/type-mangle 机制）。
- 已知局限：诊断为**同 CU 内**检测（覆盖实际撞到的同文件场景）；跨文件同 namespace 同名自由函数
  的模块级检测需 SymbolCollector 诊断布线（更大），暂不做——`_extractFunc` 加固已让那种情况也不崩。
