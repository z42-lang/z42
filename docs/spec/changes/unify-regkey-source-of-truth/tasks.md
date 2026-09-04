# Tasks: `MethodDecl.RegKey` 单一真相源（unify-regkey-source-of-truth）

> 状态：🟡 DRAFT 待审批 | 创建：2026-09-04 | 类型：refactor(compiler)，目标**字节中性**
> 见 [proposal.md](proposal.md) / [design.md](design.md)

## 阶段 0 — 审批（当前）

- [ ] 0.1 User 确认方案方向
- [ ] 0.2 User 裁决 design §3.2 的 A/B（decl-only partial 是否写 RegKey；**建议 A 案**）
- [ ] 0.3 User 裁决范围：补齐与移除兜底是否分两个 PR（**建议分开**）

## 阶段 1 — 补齐 RegKey（PR-1，纯加性、字节中性）

- [ ] 1.1 环境：worktree `../z42-regkey`（已建，基于 origin/main `44e67ef1`）+ 供种 + 基线 GREEN
- [ ] 1.2 **先取字节基线**：改动前保存 `artifacts/build/compiler/*/dist/*.zpkg` 供事后逐字节对账
- [ ] 1.3 `InheritanceResolver._passImpls`（`:33` 附近）补 `md.RegKey = md.Name;` + `isym.RegKey = md.Name;`
- [ ] 1.4 注释写明**impl 方法恒裸名、不参与 primary/非-primary 规则**（防后人误以为已统一而误改）
- [ ] 1.5 `Decl.z42:141` 的 `RegKey` 注释更新：不变量 + 明确例外（被擦除的 decl-only partial）
- [ ] 1.6 A 案下 decl-only partial 不动

### 阶段 1 验证

- [ ] 1.7 **字节对账**：改动前后 z42c 三包 zpkg 逐字节相同（BLID 除外）—— 「字节中性」的直接证明
- [ ] 1.8 `xtask test compiler`：自举不动点 gen1==gen2 3/3
- [ ] 1.9 `xtask test` 全量 GREEN（含 impl / cross-zpkg impl_propagation / impl_reflect / partial-types goldens）
- [ ] 1.10 **`xtask test stdlib --mode jit` 两 shard** —— 派发相关改动必跑 JIT，本地默认只跑 interp
- [ ] 1.11 `xtask test bootstrap` 无越界

## 阶段 2 — 移除兜底（PR-2，分批）

### 批 1：已证死代码（无前置）

- [ ] 2.1 `DeclBinder.z42:237-239`（`_bindClass`）删 ①③，只留 `md.RegKey`
- [ ] 2.2 `IrGenMemberEmitter.z42:18-20`（`EmitMethod`）同上

### 批 2：impl 对（前置 = 阶段 1 已合并）

- [ ] 2.3 `DeclBinder.z42:188-190`（`_bindImpl`）
- [ ] 2.4 `IrGenTypeEmitter.z42:122-124`（`EmitImpl`）—— **必须与 2.3 同 commit**（成对，单改任一即断链）

### 批 3：需先补测试

- [ ] 2.5 **补跨-CU partial 的 TSIG 导出测试**（现零覆盖：`src/tests/partial-types/*` 全是单文件用例）
      —— decl 碎片与 impl 碎片分处不同文件，断言两个 module 的导出面正确
- [ ] 2.6 有 2.5 兜底后再改 `ClassExtractor.z42:194-196`
- [ ] 2.7 补 `static partial` decl-only 用例 → 再改 `ClassExtractor.z42:328-330`
- [ ] 2.8 `DeclBinder.z42:336-338`（`_checkExposure`）：先对账 E0441 **重复条数**变化，
      确认无 golden 断言依赖后再改

### 阶段 2 验证

- [ ] 2.9 每批各自跑：字节对账 + 自举不动点 + 全量 GREEN + JIT 双 shard
- [ ] 2.10 批间不合并提交，红了能精确定位到批

## 明确不做

- **不改键的格式或规则**（primary 裸 / 非-primary 全签名不动）→ **无格式 bump、无两代自举**
- **不把 impl 方法并入 primary/非-primary 规则**（那会 rekey + 需格式 bump，是独立语义扩展）
- **不动 `CallEmitter.z42:238-243`** 的静态 DepIndex 查找（另一维度：静态键 + 跨版本自举容忍）

## 铁律

- 「字节中性」是本变更的核心假设。**任何一次对账不通过就立刻停下重新分析**，不要「看起来差不多就推」。
- 派发相关改动的失败模式是**静默派发到错误目标**而非崩溃 → 依赖 golden 与 JIT 模式覆盖，不能只看 interp 绿。
- 分批提交，不图省事合并。
