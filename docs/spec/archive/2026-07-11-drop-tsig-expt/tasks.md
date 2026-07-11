# Tasks: 删 TSIG/EXPT（unify-type-metadata P3）

> 状态：🟢 已完成 | 完成：2026-07-11| 创建：2026-07-11 | initiative: unify-type-metadata P3

## 进度概览（分步，每步 gate）
- [x] 步骤 1: 扩 ReadModuleTypes 读 indexed 散装 zbc（FILE + ZbcReader.Read）——indexed 包对账 OK
- [x] 步骤 2: DepScan 切 Rebuild（world 一次解析复用）→ **stdlib 字节 byte-identical vs TSIG 路径**（切换零行为变化，步骤 3 不需要）
- [x] 步骤 3: N/A（步骤 2 byte-identical，"unknown" 归一化对消费方无害，无需 IrGen 根因修）
- [x] 步骤 4: 停写 TSIG + EXPT + zpkg 30→31 + 两代自举 0.29→0.31 + regen fixtures/expected.json/header pin + 不动点 7/7
- [x] 步骤 5: 死代码清理（ReadTsig / _buildTsig / _buildExpt / _internTsig / _skipTpConstraints / Reconcile / Compare + 14 对账函数 / reconcile 单测）+ 文档 + 归档

## 实施中关键发现/修复
- **堆压力崩溃（根因修）**：初版 Rebuild 每调用重解析全 world（O(deps×world) 次包解析）→ 海量分配拖垮 interp（JIT 掩盖、interp 崩、崩点随状态漂移）。`BuildWorld` 一次解析复用 → 修复 + 构建 6min→3.5min。
- **跨包 impl 传播回归（IMPL 漏读）**：Rebuild 初版没读 IMPL 段 → impl_propagation/impl_reflect 主程序编译失败。`ReadImplInto`（原 _readImpl 公开）读进 Impls → cross-zpkg 4/4。
- 中间多次"崩溃"实为手工反复重建的脏产物污染；干净两代自举 + 干净构建始终 OK。

## 备注
- **安全网**：不动点（gen1==gen2）是切换正确性的终极 oracle——Rebuild 输出喂消费方若改变编译产物，不动点即破。
- IMPL 段保留（D2）。reconcile-tsig verb 保留为回归工具。
- 格式 bump 只 zpkg（TSIG/EXPT 是 zpkg-level，zbc 不变）。

## 实测体积收益（0.30 → 0.31，release dist zpkg）

全 29 个 release 包（22 stdlib + 7 z42c）：

| 组 | 0.30 | 0.31 | 减少 |
|----|------|------|------|
| stdlib（22 包）| 1,538,257 B (1.47 MB) | 1,121,317 B (1.07 MB) | −416,940 B (**−27.1%**) |
| z42c（7 包）| 1,080,611 B (1.03 MB) | 860,477 B (0.82 MB) | −220,134 B (**−20.3%**) |
| **合计（29 包）** | **2,618,868 B (2.50 MB)** | **1,981,794 B (1.89 MB)** | **−637,074 B (0.61 MB, −24.3%)** |

单包 top（按节省字节）：z42.core −101 KB (**−52.4%**)、z42.net −67 KB (−27.6%)、
z42.io −48 KB (−40.1%)、z42.crypto −43 KB (−15.1%)、z42.compression −21 KB (−34.7%)。

> 注：commit adec87eb 信息里「每个 zpkg ~半」是就 **z42.core**（TSIG 占比最重的包，−52%）而言；
> **全 29 包聚合约 −24%**（TSIG 体量与类型/方法签名数正相关，反射类密集的包省得最多）。
