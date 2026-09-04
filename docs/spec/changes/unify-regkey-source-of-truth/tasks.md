# Tasks: 方法注册键推导收敛到单一 owner（unify-regkey-source-of-truth）

> 状态：🟡 DRAFT 待审批 | 创建：2026-09-04 | 修订：2026-09-04（方向重定）
> 类型：refactor(compiler)，目标**字节中性** | 见 [proposal.md](proposal.md) / [design.md](design.md)

## 阶段 0 — 审批（当前）

- [ ] 0.1 User 确认方向（收敛推导逻辑，而非初版的「补字段」）
- [ ] 0.2 User 裁决范围：收敛（阶段 1）与移除回落（阶段 2）是否分两个 PR —— **建议分开**

## 阶段 1 — 收敛（PR-1，纯重构、字节中性）

- [ ] 1.1 环境：worktree `../z42-regkey`（已建，基于 origin/main `44e67ef1`）+ 供种 + 基线 GREEN
- [ ] 1.2 **先存字节基线**：改动前保存 `artifacts/build/compiler/*/dist/*.zpkg` 供事后逐字节对账

### 写侧

- [ ] 1.3 加 `_registerMethod(ct, md, sym, key)`：算键与写键绑死（design §3.1）
- [ ] 1.4 `MemberCollector._fillClass`（`:219-221`）改走它，传算好的 `regName`
- [ ] 1.5 `InheritanceResolver._passImpls`（`:33`）改走它，传 `md.Name`
      —— 这是补齐 impl 方法 RegKey 的地方
- [ ] 1.6 注释写明：**impl 方法恒裸名、不参与 primary/非-primary**；并入是独立语义扩展（会 rekey + 需格式 bump），本变更不做
- [ ] 1.7 擦除分支顺手写 `md.RegKey = regName`（`regName` 现成、恒 `md.Name`；原 B 案，见 design §3.2 注）
- [ ] 1.8 `Decl.z42:141` 的 RegKey 注释更新：不变量 + 「注册键 vs 解析键」的区分

### 读侧

- [ ] 1.9 加 `MethodKeyOf(methods, md)`（design §3.2），逐字复刻现三档语义
- [ ] 1.10 6 处查表消费点改为一行调用：`ClassExtractor:194/328`、`DeclBinder:188/237/336`、`IrGenTypeEmitter:122`
- [ ] 1.11 `IrGenMemberEmitter:18-20`（不查表）改用 `md.RegKey`，删 ①③

### 阶段 1 验证

- [ ] 1.12 **字节对账**：z42c 三包 zpkg 逐字节相同（BLID 除外）—— 不通过即停
- [ ] 1.13 `xtask test compiler`：自举不动点 gen1==gen2 3/3
- [ ] 1.14 `xtask test` 全量 GREEN
- [ ] 1.15 **`xtask test stdlib --mode jit` 两 shard**
- [ ] 1.16 `xtask test bootstrap` 无越界

## 阶段 2 — 移除回落（PR-2，分批）

阶段 1 后，`MethodKeyOf` 里的回落只对**被擦除的 decl-only partial** 生效
（1.7 落地后连这个也不走了）。逐类确认无人依赖后收窄。

- [ ] 2.1 **先补跨-CU partial 的 TSIG 导出测试** —— 现零覆盖
      （`src/tests/partial-types/*` 全是单文件用例）：decl 碎片与 impl 碎片分处不同文件，
      断言两个 module 的导出面正确。**这是 #1 的前置**
- [ ] 2.2 补 `static partial` decl-only 用例（#2 前置）
- [ ] 2.3 对账 `_checkExposure` 的 E0441 **重复条数**变化，确认无 golden 依赖（#5 前置）
- [ ] 2.4 前置齐备后收窄 `MethodKeyOf` 为「直接返回 `md.RegKey`」，或视情况整体去掉
- [ ] 2.5 每步各自跑阶段 1 的全套验证；不合并提交，红了能精确定位

## 明确不做

- **不改键的格式或规则**（primary 裸 / 非-primary 全签名不动）→ **无格式 bump、无两代自举**
- **不把 impl 方法并入 primary/非-primary 规则**
- **不动 `CallEmitter.z42:238-243`** 的静态 DepIndex 查找

## 铁律

- 「字节中性」是核心假设。**任何一次对账不通过就立刻停下重新分析**，不要「看起来差不多就推」。
- 派发相关改动的失败模式是**静默派发到错误目标**而非崩溃 → 必须跑 JIT 模式 + goldens，
  只看 interp 绿不算数。
- 分批提交，不图省事合并。
