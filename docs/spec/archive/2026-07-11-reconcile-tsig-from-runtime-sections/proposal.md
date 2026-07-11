# Proposal: TSIG 对账重建（unify-type-metadata P2）

## Why

P1-a..e 已把 TSIG 承载的全部元数据（enum 值/可见性/virtual/abstract/minArg/默认值/varargs/
参数名/delegate/impl）**加进** TYPE/SIGS/IMPL。P3 要删 TSIG/EXPT——但「编译期解析 bug 极难调」
（P1-b 亲历）。P2 是专门的安全网：z42c 新增「从 TYPE/SIGS/IMPL **重建** ExportedModuleZ」路径，
与现有 TSIG 读取**逐字段对账**（TSIG 当 oracle），在删之前证明重建正确。零行为变化。

## What Changes

- **ZpkgReader.ReadModuleSigs 停止丢弃 P1 元数据**：visibility/method_flags/min_arg/params_from/
  参数名/默认值现在只为游标对齐消费——改为灌进 IrFunction stub（字段已存在）。
- **ZpkgReader.ReadModuleTypes（新）**：解析 MODS 每模块记录的 TYPE 字节 → IrClassDesc[]
  （复用/公开 ZbcReader 的 TYPE 解析）。
- **TsigReconcile（新模块）**：`Rebuild(z)` 从 (types, sigs, impls) 重建 ExportedModuleZ[]，
  含归一化（FQ→裸名/短基名、参数名→"p{i}"/"arg{i}"、u8→可见性串、arity demangle、成员
  纳入规则镜像 ExportedTypeExtractor）；`Compare(oracle, rebuilt)` 逐字段报差异。
- **driver verb `z42c reconcile-tsig <zpkg>...`**：每包打印 OK/差异明细，任一差异 exit 1。
- **验证**：对全 29 包（stdlib 22 + z42c 7）跑 verb 全 OK（本 change 验收）。CI gate 布线
  需 toolchain 锁（被占）→ 排队为 follow-up；P3 全面切换后整个 GREEN gate 即覆盖。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | ReadModuleSigs 灌 P1 元数据；ReadModuleTypes 新增 |
| `src/compiler/z42c.project/src/ZpkgWriterIndexed.z42` | MODIFY | `_internSigStrings` 补 P1-d 参数名 + str 默认值入池（P1-d 遗漏——indexed 无 MODS 体须显式入池；ReadModuleSigs 改 store-deref 后 0xFFFFFFFF 崩溃暴露）|
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | TYPE 解析公开入口（ReadTypeSection(bytes,pool)，factor 自现有私有） |
| `src/compiler/z42c.project/src/TsigReconcile.z42` | NEW | Rebuild + 归一化 + Compare |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | **对账 gate 揪出的 SIGS 精度根因修**：native 桩真实 ret/参数类型（原硬编码 "object"）+ 5 合成点补逻辑 MinArg（property/indexer/synth-ctor）+ auto-prop 后备字段 Visibility=private。P3「SIGS 作唯一真相」的前置精度修正 |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `reconcile-tsig` verb 分发 |
| `src/compiler/z42c.project/tests/reconcile/*` | NEW | 单测（人造包重建对账） |
| `docs/design/compiler/*.md` 或 `docs/book/` | MODIFY | 重建机制文档 |

**只读引用**：`ExportedTypeExtractor.z42`（纳入规则/命名口径镜像源）、`ZpkgWriter.z42`
（TSIG 布局）、`docs/spec/changes/unify-type-metadata/design.md`。

## Out of Scope
- 删 TSIG/EXPT（P3）；xtask/CI gate 布线（toolchain 锁被占，排队）
- 行为切换（本 change 后 z42c 仍读 TSIG）

## Open Questions
- [ ] 归一化细节（"p{i}" vs 源名等）在实施中以 oracle 实测为准迭代——差异即 bug 或归一化缺口，逐项收敛
