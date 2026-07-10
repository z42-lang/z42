# Tasks: 跨包 impl 反射（P1-e ①）

> 状态：🟢 已完成 | 完成：2026-07-11（/loop 批量授权「一直推进到 P2/P3 完成」）| 创建：2026-07-11 | initiative: unify-type-metadata P1-e

## 进度概览
- [x] 阶段 1: VM 解析 + 注册表（zbc_reader/loader/lazy_loader/vm_context/main）
- [x] 阶段 2: 反射（builtin_type_interfaces 并入 impl traits）
- [x] 阶段 3: 测试（单测 + cross-zpkg e2e）+ 全 GREEN
- [x] 阶段 4: 文档 + 归档

## 阶段 1: VM
- [x] 1.1 zbc_reader `read_zpkg_impl_pairs(raw)`（dir+STRS+IMPL；方法 17+pc×8 跳过器）
- [x] 1.2 LoadedArtifact.impl_pairs + packed/indexed 填充
- [x] 1.3 LazyLoader.impls + load_zpkg_file 合并（追加语义）+ impl_traits_for + seed_impls
- [x] 1.4 VmContext.impl_traits_for 转发 + main.rs 主模块 seed

## 阶段 2: 反射
- [x] 2.1 builtin_type_interfaces：base 链每类 queue += impl traits

## 阶段 3: 测试 + GREEN
- [x] 3.1 read_zpkg_impl_pairs 单测（人造字节）
- [x] 3.2 cross-zpkg e2e fixture `impl_reflect`
- [x] 3.3 全 GREEN（xtask test）+ cargo

## 阶段 4: 文档 + 归档
- [x] 4.1 reflection.md 跨包 impl 反射节 + zpkg.md IMPL 消费者
- [x] 4.2 归档 + ACTIVE.md 释放 + commit/push + 盯 CI

## 备注
- 无格式 bump / 无两代自举 / z42c 零改动（IMPL 读现有段）。
- /loop 批量授权模式：P1-e①→P1-e②(delegate)→P2→P3 连续推进，中断条件照常。
