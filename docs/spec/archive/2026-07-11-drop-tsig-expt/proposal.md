# Proposal: 删 TSIG/EXPT——TYPE/SIGS/IMPL 成唯一元数据（unify-type-metadata P3）

## Why

P1 把 TSIG 独有字段全补进 TYPE/SIGS/IMPL；P2 证明可无损重建（29 包 0 DIFF）。P3 兑现收益：
**z42c 跨包解析改读 TYPE/SIGS/IMPL（经 TsigReconcile.Rebuild），停写 TSIG + EXPT，删两段**。
每个 zpkg 变小、元数据单一真相、消除"两份重复副本"。

## What Changes

- **消费侧切换**：`DepScan` 从 `ZpkgReader.ReadTsig(z)` 改为 `TsigReconcile.Rebuild(z, world)`
  （world = 全部 dep zpkg，跨包 base 链）。
- **indexed TYPE 读取**（User 定：扩 ReadModuleTypes）：indexed dep 的 TYPE 在散装 .zbc → 经 FILE
  目录 + 包目录路径逐个 load、ReadTypeAt 提取（各 zbc 自包含局部池）。
- **停写**：ZpkgWriter / ZpkgWriterIndexed 不再 emit TSIG + EXPT 段；`_buildTsig`/`_buildImpl` 中
  TSIG 部分删（IMPL 保留，D2）。段序减两段。
- **删读**：`ZpkgReader.ReadTsig` + EXPT 相关；`ExportedTypeExtractor` 是否仍需（TSIG 生产端）——
  Rebuild 取代其 zpkg 侧，但**本包自身**导出仍可能需其产 ExportedModule (IrDump.Exported)；评估保留。
- **格式 bump**：zpkg 0.30→0.31（段面减 TSIG/EXPT）。zbc 不变（TSIG/EXPT 是 zpkg-level）。
- **Rust**：EXPT 是 zpkg 段，Rust reader 若引用 SEC_EXPT 需清理；TSIG Rust 不读。

## 分步实施（风险前置消除）

1. **扩 ReadModuleTypes（indexed）** → 单测。
2. **切 DepScan 到 Rebuild**（**保留 TSIG 写入**）→ GREEN + 自举不动点作**经验 oracle**：
   验证 Rebuild 输出喂给消费方后编译无变化（归一化差异——unknown/internal/subset——是否真无害）。
3. 若不动点/GREEN 破 → 定位消费方敏感点（很可能 `_resolve("unknown")`）→ 根因修 IrGen
   （TYPE/SIGS 发真实类型名替代 "unknown"）→ 重验。
4. 全绿后：**停写 TSIG + EXPT + 格式 bump**（两代自举）→ 最终 GREEN。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | ReadModuleTypes 扩 indexed（FILE + 散装 zbc）；删 ReadTsig |
| `src/compiler/z42c.pipeline/src/DepScan.z42` | MODIFY | ReadTsig → Rebuild(world) |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | 停写 TSIG + EXPT；段序 |
| `src/compiler/z42c.project/src/ZpkgWriterIndexed.z42` | MODIFY | 停写 TSIG + EXPT |
| `src/compiler/z42c.project/src/ZpkgWriterZ.z42` 版本 | MODIFY | Minor 30→31 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | （条件）TYPE/SIGS 发真实类型名替代 "unknown"（若步骤 3 需要）|
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | ZPKG_VERSION_MINOR 30→31 + EXPT 清理（如引用）|
| `src/runtime/src/metadata/formats.rs` | MODIFY | SEC_EXPT 清理（如无其他消费）|
| `src/compiler/z42c.project/tests/reconcile/` 或新 | MODIFY | ReadModuleTypes indexed 单测 |
| `src/tests/zpkg-format/*` | MODIFY | regen（段面变）|
| `src/compiler/z42c.project/tests/zpkg/zpkg_tests.z42` | MODIFY | header pin 31 + 段序 golden |
| `docs/design/runtime/zpkg.md` / `.claude/rules/version-bumping.md` | MODIFY | changelog + 常量表 + 段面 |
| `docs/design/compiler/project.md` | MODIFY | TSIG 对账节 → 收口为「已删 TSIG，Rebuild 是唯一路径」 |

**只读引用**：`ImportedSymbolLoader.z42`（消费方 `_resolve` 口径）、`TsigReconcile.z42`（Rebuild）、
`docs/spec/changes/unify-type-metadata/design.md`（P3 定义）。

## Out of Scope
- ExportedTypeExtractor 彻底删除（若本包导出仍用它，保留；只删 zpkg 侧 TSIG 读写）
- reconcile-tsig verb 删除（保留为 TYPE/SIGS 健康度回归工具）

## Open Questions
- [ ] 步骤 2 经验验证结果决定是否需步骤 3 的 IrGen "unknown" 根因修（实测驱动）
