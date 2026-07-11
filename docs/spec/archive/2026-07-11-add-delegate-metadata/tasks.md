# Tasks: delegate 元数据（P1-e ②）

> 状态：🟢 已完成 | 创建：2026-07-11 | 完成：2026-07-11 | initiative: unify-type-metadata P1-e

## 进度概览
- [x] 阶段 1: z42c（DelegateDecl → TYPE bit6+tps + Invoke 死体桩；bump 26/30）
- [x] 阶段 2: VM（CLASS_FLAG_DELEGATE + is_delegate + __type_is_delegate + Type.IsDelegate）
- [x] 阶段 3: 两代自举 + regen + golden/pins
- [x] 阶段 4: 测试 + 全 GREEN + 文档 + 归档

## 阶段 1: z42c
- [x] 1.1 IrGen DelegateDecl pass（IrClassDesc：_q 名 + Flags 0x40 + TypeParams；镜像 enum pass）
- [x] 1.2 IrGen `_emitDelegateInvoke`（合成 MethodDecl "public virtual" → 死体桩 + 源拼写参数类型 + _fillParamMeta/_methodFlags）
- [x] 1.3 ZbcFormat 25→26 + ZpkgWriter 29→30

## 阶段 2: VM
- [x] 2.1 bytecode CLASS_FLAG_DELEGATE + types.rs is_delegate()
- [x] 2.2 reflection.rs __type_is_delegate + mod.rs 注册 + Type.z42 IsDelegate
- [x] 2.3 zbc_reader 常量 26/30 + changelog

## 阶段 3: bump 流程
- [x] 3.1 快照 0.29 种子 + 两代自举 0.29→0.30（gen1-stdlib EMPTY Z42_LIBS）
- [x] 3.2 regen zbc-format 6 + zpkg-format 4 + golden hex + header pins + Rust pinned 26/30 + expected.json

## 阶段 4: 测试 + 文档 + 归档
- [x] 4.1 reflection.z42 [Test]（IsDelegate + Invoke 签名）
- [x] 4.2 全 GREEN + 不动点 + cargo
- [x] 4.3 reflection.md（delegate 反射节 + Type 成员表 + typeof(delegate) Deferred + roadmap 索引）+ zbc/zpkg changelog + version-bumping 表
- [x] 4.4 归档 + 释放锁 + commit/push + 盯 CI

## 备注
- D1：泛型 tps 存 TYPE、Invoke 按名引用（IrFunction 无 tp 写支持）。D3：不动 ResolveTypeP，typeof(delegate) Deferred。
- P3 硬前置：TSIG 携带 delegate（含泛型内建 11 个）→ 删 TSIG 前须可从 TYPE/SIGS 重建。
