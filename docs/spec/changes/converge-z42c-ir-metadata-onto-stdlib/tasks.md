# Tasks: z42.ir 收敛（IR + zbc + zpkg 后端合一入 stdlib）

> 状态：**APPROVED（User 定：合并单库 z42.ir + CacheStore 留构建侧）** | 创建：2026-07-21
> 每阶段独立 GREEN（self-host 7/7 byte-identical + test compiler）才 commit。

## 阶段 A：落 z42.ir（并存自测，不进 z42c libs）
- [ ] A1 `src/libraries/z42.ir/src/`：拷 z42c.ir 全部 + z42c.project 后端 6 文件（ZpkgReader/Writer/WriterIndexed/Builder/PackageTypes/TsigReconcile；**不含 CacheStore**）
- [ ] A2 `z42.ir.z42.toml`（kind=lib；deps: z42.encoding, z42.io, z42.crypto；namespace 三段不变）
- [ ] A3 `tests/ir-roundtrip/`：zbc write→read + zpkg round-trip [Test]
- [ ] A4 进 `src/libraries/z42.workspace.toml` default-members（z42.crypto 之后）
- [ ] A5 验证：`z42.ir.zpkg` 编出 + [Test] 绿；**不**进 z42c 构建 libs（防串味）

## 阶段 B：z42c 切 z42.ir + 删旧包 + 迁 CacheStore（同一原子提交）
- [ ] B1 z42c.{semantics,pipeline,driver}/*.z42.toml：deps 去 `z42c.ir`+`z42c.project`，加 `z42.ir`
- [ ] B2 `CacheStore.z42` 迁 `src/compiler/z42c.pipeline/src/`（namespace `Z42.Project` 不变，消费者零改）
- [ ] B3 删 `src/compiler/z42c.ir/` + `src/compiler/z42c.project/` + `src/compiler/z42.workspace.toml` 两 member
- [ ] B4 验证：self-host 7/7 byte-identical + test compiler 21 单元全绿

## 阶段 C：CI 拓扑 + 文档
- [ ] C1 ci.yml：stdlib 构建含 z42.ir（拓扑序在 z42c.* 前）；`xtask test bootstrap` 核对（纯包重组无新语法/格式 → 应过）
- [ ] C2 文档：compiler-architecture（IR/zpkg 归属改述）+ project.md + doc-system 索引
- [ ] C3 **converge-z42c-onto-z42-project/design 决策 1 更正**（zpkg 后端下沉 z42.ir，作废 z42c.zpkg）
- [ ] C4 ACTIVE.md 释放 compiler + stdlib 锁；roadmap.md:122 勾除

## GREEN 门（每阶段）
- self-host `--workspace` gen1==gen2 逐字节 7/7 · test compiler 21 单元 · warm 本地可验，cold/CI 以 ci-bootstrap 为权威

## 已定（User）
- 合并为**单库 z42.ir**（非 z42.ir + z42.metadata 两库）
- CacheStore **留构建侧** → 迁 z42c.pipeline
- 规范张力（zpkg 后端 compiler→stdlib）以本 change 为准，更正 converge design 决策 1
