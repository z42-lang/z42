# Tasks: z42.ir 收敛（IR + zbc + zpkg 后端合一入 stdlib）

> 状态：**A+B 已落地（commit f1cbcf9d），C 收尾中** | 创建：2026-07-21
> 每阶段独立 GREEN（self-host byte-identical + test compiler）才 commit。

## 阶段 A：落 z42.ir（并存自测）✅
- [x] A1-A5 z42.ir 库（23 src：IR 模型 + zbc + zpkg 后端 6 文件，不含 CacheStore）+ toml + smoke [Test] + workspace member。冒烟 3/3 绿。

## 阶段 B：z42c 切 z42.ir + 删旧包 + 迁 CacheStore（原子 f1cbcf9d）✅
- [x] B1 z42c.{semantics,pipeline,driver} deps → z42.ir
- [x] B2 CacheStore 迁 z42c.pipeline（namespace Z42.Project 不变）
- [x] B3 删 z42c.ir + z42c.project + compiler workspace 两 member（现 5 member）
- [x] B4 验证：self-host **5/5** byte-identical（z42c）+ z42.ir 稳定；test compiler **23 单元 336 tests** 全绿（含重定位 depindex/zpkg→z42.ir、zbcreader→z42c.semantics、新 smoke）

## 阶段 C：CI 拓扑 + 文档
- [x] C1 CI：`_compilerMembers` 动态读 workspace（自适应 5）；修 jit-fixpoint-check 硬编码 member 列表 + ci-bootstrap 的 ZpkgWriter.z42 路径（→ z42.ir）。stdlib 构建经 workspace default-members 自动含 z42.ir（纯包重组无格式 bump → bootstrap 应过，以 CI 为权威）
- [x] C3 converge-z42c-onto-z42-project/design 决策 1 更正（zpkg 后端下沉 z42.ir，作废 z42c.zpkg）
- [ ] C2 文档：compiler-architecture / project.md 结构描述（z42c 现 5 子包 + z42.ir）——待补
- [ ] C4 ACTIVE.md 释放锁；roadmap.md:122 勾除——待补

## GREEN 门（每阶段）
- self-host `--workspace` gen1==gen2 逐字节 7/7 · test compiler 21 单元 · warm 本地可验，cold/CI 以 ci-bootstrap 为权威

## 已定（User）
- 合并为**单库 z42.ir**（非 z42.ir + z42.metadata 两库）
- CacheStore **留构建侧** → 迁 z42c.pipeline
- 规范张力（zpkg 后端 compiler→stdlib）以本 change 为准，更正 converge design 决策 1
