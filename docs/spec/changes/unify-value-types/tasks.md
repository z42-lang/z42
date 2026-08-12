# Tasks: unify-value-types Phase 1（编译器核心 —— 消灭 Z42PrimType）

> 状态：🟡 进行中 | 创建：2026-08-12
> 伞程序 `unify-value-types` 覆盖 R1-R7 分 4 阶段（见 proposal）；**本 tasks 只列 Phase 1**（纯编译器、
> 零格式 bump、self-host byte-identical 门禁）。后续 Phase 2-4 各自开独立 change。

## 进度概览
- [x] 阶段 0: 环境准备（worktree 播种）
- [x] 阶段 1: PrimModel 单一表 + StructLayout.ReprOf（新增，不删旧）
- [ ] 阶段 2: 消费点逐个切到 PrimModel（每切一处验 self-host 不变）
- [ ] 阶段 3: 删 Z42PrimType 类 + 旧七表 + 消 sentinel
- [ ] 阶段 4: 测试文件机械替换 + 新单测
- [ ] 阶段 5: 文档同步（book 机制页 + roadmap）
- [ ] 阶段 6: GREEN + 归档 Phase 1 + PR

## 阶段 0: 环境准备
- [x] 0.1 worktree z42-uvt 播种：`cp -R <SDK 0.37> .z42`（SDK 从 `gh run download <近期 runid> -n current-sdk-macos-15`），`cargo build --release` 建 z42vm，用种子建 xtask.zpkg，`xtask build compiler` warm 一把过验环境（见 [[fresh-worktree-seed-setup]]）
- [x] 0.2 基线：改动前跑 `xtask test compiler` 确认 self-host 5/5 gen1==gen2（拿到基线产物 sha 供后续对账）

## 阶段 1: PrimModel 单一表 + Repr（新增，旧七表暂留）
- [x] 1.1 `StructLayout.z42`（或新辅助）：`PrimModel` 表——keyword→{canonName, fqName, irTag, repr, isNumeric, isInteger}；覆盖 int/long/short/byte/sbyte/uint/ulong/ushort/float/double/bool/char + Std.* FQ
- [x] 1.2 `StructLayout.ReprOf(name)`：wrapper→Scalar / IsBlobStruct→Blob / 其余→N/A
- [x] 1.3 单测：PrimModel 每个 keyword 的五字段 + ReprOf 分类（tests/types）

## 阶段 2: 消费点切到 PrimModel（每步 `xtask test compiler` 验 byte-identical）
- [ ] 2.1 `SymbolTable.ResolveTypeP`（:113）：基元分支改产 `Std.*` Z42ClassType（从符号表查 phantom struct）；`_isPrim`/`_canonPrim` 内部改读 PrimModel
- [ ] 2.2 `EmitContext.ToIrType`（:508）/`PrimTag`：`is Z42PrimType`→"Scalar 值类型→PrimTag(canonName)"；`_primWrapper` 读 PrimModel
- [ ] 2.3 `TypeChecker.BoxIfNeeded`（:41）/`_intPrimFQ`：`is Z42PrimType`→Repr 判定；整数→__box_prim、bool/char/double 不装箱特例保留
- [ ] 2.4 `Z42Type.Canon` / `IsAssignableTo` / `_canWiden`：拓宽/窄化迁到统一值类型可赋性（D7），读 PrimModel
- [ ] 2.5 字面量 typing 产出：`ExprTyper.z42`（int/long/char/string/bool/内插/拼接）、`TypeFactsTc.z42`（float/double）、`MemberResolver.z42`（.Length/.Count/枚举常量）、`SymbolCollector.z42`（object 基类方法签名）、`BinaryTypeTable.z42`（算术结果）、`Bound.z42`（BoundIsExpr bool）→ 全改产 Std.* 值类型
- [ ] 2.6 `ConstraintChecker._isStructArg`（:159）：`is Z42PrimType`→"IsStruct 值类型"
- [ ] 2.7 `FunctionEmitter.z42`（:81）：形参 PrimTag 判别改 Repr
- [ ] 2.8 `ExprEmitter.z42`：packed 数组元素判别（:79/111）+ `_emitBox` 透传（:1367）改按 Repr；确认 `_emitBinary` 未动

## 阶段 3: 删旧 + 根因修
- [ ] 3.1 删 `Z42PrimType` 类（Z42Type.z42:32-49）+ 确认无残留引用（`grep -rn Z42PrimType src/compiler`）
- [ ] 3.2 删旧七表冗余投影（`_canonPrim`/`_primWrapper`×2/`_intPrimFQ`/`_isPrim`×3/`_isPrimKeyword`/`_isNumericPrim` 归并进 PrimModel）
- [ ] 3.3 `ImportedSymbolLoader.z42`（:356）：删 `Z42PrimType` 降级 sentinel（根因修，解析到正确值类型）；若根因在 Scope 外→停下报告 User

## 阶段 4: 测试
- [ ] 4.1 `tests/{types,bound,map,overload}/*.z42`：`new Z42PrimType(...)` 机械替换为查 Std.* 值类型 / 删除
- [ ] 4.2 新单测：ResolveType("int") 返回 Z42ClassType(Std.Int32, IsStruct, Repr=Scalar)；可赋性/拓宽/重载回归

## 阶段 5: 文档同步
- [ ] 5.1 `docs/book/src/runtime/value-type-model.md`（NEW）：统一值类型模型（Repr/Scalar/Blob/七表收敛/codegen 不变式/与 struct-value-semantics 关系）+ 挂 SUMMARY.md
- [ ] 5.2 `docs/roadmap.md`：unify-value-types 程序进度 + Phase 2-4 Deferred 索引
- [ ] 5.3 目录 README 六段核对（z42c.semantics 若入口/功能索引变化）

## 阶段 6: 验证 + 归档 + PR
- [ ] 6.1 `cargo build --release`（z42vm，应无关；确认不破）
- [ ] 6.2 `xtask test`（**不传 Z42_HOME**）完整 GREEN gate：e2e / cross-zpkg / stdlib / compiler(self-host 5/5 byte-identical) / vscode-syntax
- [ ] 6.3 `xtask test bootstrap`：确认无越界（无新语法/格式）
- [ ] 6.4 spec scenarios 逐条覆盖确认
- [ ] 6.5 归档 `docs/spec/changes/unify-value-types/`（Phase 1 完成后视伞程序策略：或保留待后续阶段）+ commit（含 .claude 记忆 + docs/spec）
- [ ] 6.6 rebase origin/main + 重跑 GREEN → PR（格式中立 CI 应干净过；分支保护 User 手动合 [[stale-required-check-blocks-pr-merge]]）
- [ ] 6.7 更新 memory 程序文件 + 续推口令

## 备注
- **codegen-output-preserving 是主门禁**：每个阶段 2 子步都跑 `xtask test compiler` 验 self-host byte-identical；任何漂移即 bug，修到不动点（或 D7 一代自愈，如过往 opt 改动）。
- **不传 Z42_HOME**（血泪教训）；worktree 播种见 [[fresh-worktree-seed-setup]]。
- 实施中若发现 root cause 在 Scope 外 / 决策点未覆盖 → 停下报告（越界防护 + 设计完整性）。
