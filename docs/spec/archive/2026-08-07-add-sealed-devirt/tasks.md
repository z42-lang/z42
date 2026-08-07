# Tasks: 基于 sealed 的去虚化

> 状态：🟢 已完成 | 完成：2026-08-07| 创建：2026-08-07
> 分支/worktree：`add-sealed-devirt` @ `/Users/d.s.qiu/Documents/codesigner-ui/z42-devirt`（基于 origin/main 17af9ac3，含 sealed 地基）
> follow-up of `impl-sealed-semantics`（#140）

## 进度概览
- [ ] 阶段 1: 目标解析 + devirt emit（EmitContext + ExprEmitter + OptSet）
- [ ] 阶段 2: 测试矩阵（e2e 对拍 + codegen 单测）
- [ ] 阶段 3: 文档 + 验证 + 归档

## 阶段 1: 核心实现
- [ ] 1.1 `OptSet.z42`：新增 `Opt.Devirt` 位（入 `All`；`ByName` "devirt"）
- [ ] 1.2 `EmitContext.z42`：`SealedReceiverClass(recvType)` —— recvType 是本地非泛型 sealed 类 → 返回 Z42ClassType，否则 null
- [ ] 1.3 `EmitContext.z42`：`ResolveSealedTarget(ct, method, argc)` —— 沿基链找最近声明（非 abstract、本地非泛型）→ `QualifyClass(定义类)+"."+RegKey`；越界返回 ""
- [ ] 1.4 `ExprEmitter._emitCall`（:730 instance 分支，:766 VCall 前）：`Opt.Devirt` 门控 + `SealedReceiverClass` + 非 cast-Unknown + `ResolveSealedTarget` 非空 → 发直接 `CallInstr`（recv 前置）
- [ ] 1.5 确认目标名逐字节匹配 IrGen 命名（`_q(_classIrShortName)+"."+RegKey`）——本地同 ns 已验等价；跨 ns 定义类不确定则返回 ""

## 阶段 2: 测试矩阵
- [ ] 2.1 e2e `sealed_devirt/local_sealed_declared`（+ expected）——sealed 自身声明 virtual，结果正确
- [ ] 2.2 e2e `sealed_devirt/local_sealed_inherited`（+ expected）——继承未 override → 目标基类实现
- [ ] 2.3 e2e `sealed_devirt/nonsealed_stays_vcall`（+ expected）——非 sealed → override 生效（未误去虚化）
- [ ] 2.4 **before/after 对拍**：2.1–2.3 各跑 `--no-opt devirt` 开/关，stdout 逐字节一致（主正确性门）
- [ ] 2.5 codegen 单测：sealed→`call @Ns.Cls.M`（非 vcall）；非 sealed→vcall；继承→`call @Ns.Base.M`
- [ ] 2.6 内联验证：sealed 调用被 IrInline 展开（dump）
- [ ] 2.7 回落安全：泛型/imported sealed receiver → 仍 vcall

## 阶段 3: 验证 + 文档
- [ ] 3.1 `cargo build` + 完整 `xtask test`（含 z42c 自举不动点；devirt 改 IR → 破一代 warm 重建自愈）
- [ ] 3.2 spec scenarios 逐条覆盖确认
- [ ] 3.3 `docs/book/src/language/sealed.md`：去虚化从 Deferred → 机制/实现节（v1 规则 + 与 PIC 分工）
- [ ] 3.4 `docs/design/runtime/optimization-pipeline.md`：devirt pass 描述
- [ ] 3.5 `z42c.semantics/README.md` 功能索引 + `docs/roadmap.md` Deferred（本项落地 + imported/泛型/sealed-override 为新 Deferred）
- [ ] 3.6 归档 doc-check + move → `docs/spec/archive/YYYY-MM-DD-add-sealed-devirt/`

## 备注
- **无格式 bump**（复用 CallInstr，零 IR/zbc 变化）→ 不涉及两-nightly / fixture 重生。
- **两-nightly 无关**：devirt 不在 z42c/stdlib 源码使用 sealed（它 emit IR，不写 sealed 语法）；
  但 v1 只本地 sealed，imported sealed 去虚化需 sealed 元数据进已发布 nightly 才有意义 → 属 Deferred 的天然前置。
- **正确性铁律**：目标名错=静默 miscall；主门是 `--no-opt devirt` before/after 逐字节对拍。
