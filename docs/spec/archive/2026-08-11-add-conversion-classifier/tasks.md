# Tasks: 统一类型转换分类器（PR1）

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11

## 进度概览
- [x] 阶段 1: 分类器实现
- [x] 阶段 2: 接入 _isAssignable（byte-identical）
- [x] 阶段 3: 测试与文档
- [x] 阶段 4: 验证（自举字节不动点 + 全 GREEN）

## 阶段 1: 分类器实现
- [x] 1.1 新建 `Conversion.z42`：`ConvKind` 常量集（static class + int，12 项）
- [x] 1.2 `ConvResult` sealed class：`Kind` + `Method`(null) + `ImplicitOkPermissive()` + `Exists()`
- [x] 1.3 数值矩阵：`_numericKind(from, to)` → Identity / ImplicitNumeric / ExplicitNumeric（尾数位宽判有损浮点）
- [x] 1.4 `Conversion.Classify(from, to, symbols)`：按 design 分支序（镜像旧 _isAssignable）产 ConvResult

## 阶段 2: 接入（byte-identical）
- [x] 2.1 `TypeFactsTc._isAssignable` 改为 `Conversion.Classify(from,to,symbols).ImplicitOkPermissive()`
- [x] 2.2 逐分支比对：确认新投影与旧 `_isAssignable` 每个 if 分支布尔等价（error/泛型/object/数值/继承/接口）

## 阶段 3: 测试与文档
- [x] 3.1 `tests/conversion/conversion_tests.z42` + `.toml`：覆盖 spec 全部 Scenario（9 单测全 PASS；注意 `impl` 是关键字不可作变量名）
- [x] 3.2 新建 book 机制页 `docs/book/src/compiler/type-conversion.md`（分类矩阵 + 三 PR 路线）+ 挂 SUMMARY.md
- [x] 3.3 更新 `z42c.semantics/README.md` 核心文件表登记 Conversion.z42

## 阶段 4: 验证
- [x] 4.1 `cargo build --release`（z42vm）—— 无编译错误（gate regen 波内）
- [x] 4.2 `xtask test compiler` —— 自举 5/5 + **gen1==gen2 字节不动点**（核心验证 ✅）
- [x] 4.3 `xtask test e2e` + `e2e --dir cross-zpkg` + `stdlib`(280/22) —— 全 golden 输出**零变化**
- [x] 4.4 `xtask test vscode-syntax` —— 一致
- [x] 4.5 spec scenarios 逐条覆盖确认（9 单测 ↔ spec Scenario）
- [x] 4.6 文档同步核对（README / book / SUMMARY）

## 备注
- 完整 `xtask test` FULLGATE_EXIT=0 全绿；自举不动点 5/5 gen1==gen2 证明 byte-identical。
- 后续：PR2（收紧门 + 插 ConvertInstr + 迁移 stdlib/z42c 源）、PR3（用户自定义 implicit/explicit + C# 改进）。

## 备注
- byte-identical 是硬约束：任何 golden 变化或 gen1≠gen2 → 分类器分支序/判定与旧 _isAssignable 有偏差，修回等价（非改 golden）。
- PR1 不碰 cast 绑定 / BoxIfNeeded / codegen / 运行期；不加诊断码。
