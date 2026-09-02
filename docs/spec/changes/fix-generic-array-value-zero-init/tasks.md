# Tasks: 泛型数组值类型零初始化根修（方案 C）

> 状态：🟡 DRAFT（待 User 确认后进 IMPL）| 创建：2026-09-02

## 进度概览
- [ ] 阶段 0: DRAFT 确认（User）
- [ ] 阶段 1: IR + 格式（zbc bump）
- [ ] 阶段 2: codegen（emit 操作数）
- [ ] 阶段 3: VM（array_new 解析操作数 → 零值）
- [ ] 阶段 4: stdlib 去绕过 + 测试
- [ ] 阶段 5: 验证 + 文档同步

## 阶段 1: IR + 格式
- [ ] 1.1 `IrInstr.z42` `ArrayNewInstr` 加 `TypeParamKind` + `TypeParamIndex`
- [ ] 1.2 `ZbcInstr.z42` writer 编码新字段
- [ ] 1.3 `ZbcReaderInstr.z42` reader 对称解码
- [ ] 1.4 `ZbcFormat.z42` zbc Minor 36→37 + changelog；`ZpkgWriter.z42` zpkg 41→42 + changelog
- [ ] 1.5 `versions.rs` ZBC_VERSION_MINOR 37 / ZPKG_VERSION_MINOR 42 + changelog
- [ ] 1.6 `bytecode.rs` `ArrayNewInsn` 加字段 + 反序列化对称

## 阶段 2: codegen
- [ ] 2.1 `ExprTyper._bindArrayNew`：泛型形参解析 (kind, ParamIndex)（复用 default(T) 逻辑）
- [ ] 2.2 `ExprEmitter` ArrayNew emit (kind, index)；非泛型元素 (0, -1)

## 阶段 3: VM
- [ ] 3.1 `exec_array.rs array_new`：kind!=0 → method_type_args/type_args → 具体类型名
- [ ] 3.2 复用 `default_value_for` + `pack_backing` / `try_struct_backed` 产零值+backing
- [ ] 3.3 ArrayObj.element_type = 具体类型名（修反射元素类型）
- [ ] 3.4 class 级接收者 type_args 获取（参照 default_of；或按 Decision 5 降级）
- [ ] 3.5 `exec_array_tests.rs` VM 单测

## 阶段 4: stdlib 去绕过 + 测试
- [ ] 4.1 `Array.z42` Resize 移除显式填尾
- [ ] 4.2 `tests/generic_array_zero_init.z42` 端到端（spec S 全覆盖）

## 阶段 5: 验证 + 文档同步
- [ ] 5.1 `cargo build --release` + `cargo test --lib`（runtime）
- [ ] 5.2 完整 `xtask test` 全绿（含自举不动点——kind=0 编码不变保证 gen1==gen2）
- [ ] 5.3 book：array/zbc 机制页记零初始化 + 操作数编码（知识上浮）
- [ ] 5.4 冷路径格式-bump 以 CI 为准（两代自举，回归已修 #383/#385）

## 备注
- 前置（CI 两代自举格式-bump 回归）**已解除**（#383 合并 + #385 复验），无独立前置。
- kind=0 编码不变是自举字节安全的关键：非泛型数组 zbc 不漂移。
- Decision 5：class 级可选降级（首版可只 method 级，格式仍预留 kind=2）。
