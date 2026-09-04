# Tasks: 方法注册键推导收敛到单一 owner（unify-regkey-source-of-truth）

> 状态：🟢 阶段 1 实施+验证完成（阶段 2 另 PR）| 创建：2026-09-04 | 修订：2026-09-04（方向重定）
> 类型：refactor(compiler)，目标**字节中性** | 见 [proposal.md](proposal.md) / [design.md](design.md)
>
> **阶段 2 已完成**（2026-09-05，另 PR）：读侧回落删除 → `MethodKeyOf` 收窄为不变量守卫，
> 并补齐下面「阶段 2」列出的两块零覆盖测试。详见
> [`archive/2026-09-05-unify-regkey-phase2`](../2026-09-05-unify-regkey-phase2/tasks.md)。

## 阶段 0 — 审批（当前）

- [x] 0.1 User 确认方向（收敛推导逻辑，而非初版的「补字段」）
- [x] 0.2 User 裁决：收敛（阶段 1）与移除回落（阶段 2）**分两个 PR**

## 阶段 1 — 收敛（PR-1，纯重构、字节中性）

- [x] 1.1 环境：worktree `../z42-regkey`（已建，基于 origin/main `44e67ef1`）+ 供种 + 基线 GREEN
- [x] 1.2 **先存字节基线**：改动前保存 `artifacts/build/compiler/*/dist/*.zpkg` 供事后逐字节对账

### 写侧

- [x] 1.3 加 `_registerMethod(ct, md, sym, key)`：算键与写键绑死（design §3.1）
- [x] 1.4 `MemberCollector._fillClass`（`:219-221`）改走它，传算好的 `regName`
- [x] 1.5 `InheritanceResolver._passImpls`（`:33`）改走它，传 `md.Name`
      —— 这是补齐 impl 方法 RegKey 的地方
- [x] 1.6 注释写明：**impl 方法恒裸名、不参与 primary/非-primary**；并入是独立语义扩展（会 rekey + 需格式 bump），本变更不做
- [x] 1.7 擦除分支顺手写 `md.RegKey = regName`（`regName` 现成、恒 `md.Name`；原 B 案，见 design §3.2 注）
- [x] 1.8 `Decl.z42:141` 的 RegKey 注释更新：不变量 + 「注册键 vs 解析键」的区分

### 读侧

- [x] 1.9 加 `MethodKeyOf(methods, md)`（design §3.2），逐字复刻现三档语义
- [x] 1.10 6 处查表消费点改为一行调用：`ClassExtractor:194/328`、`DeclBinder:188/237/336`、`IrGenTypeEmitter:122`
- [x] 1.11 `IrGenMemberEmitter:18-20`（不查表）改用 `md.RegKey`，删 ①③

### 阶段 1 验证

- [x] 1.12 **字节对账（做法比原计划更强）**：直接比 z42c 自身产物意义不大——改了它的源，
      它自己的 zpkg 当然变。**决定性做法 = A/B**：用**改动前**的 z42c 与**改动后**的 z42c
      各编一遍**未改动**的 `z42.core`，产物**逐字节相同** ✅ —— 证明 helper 与那 7 份手抄
      逐字等价。（另：7 个 z42c 产物里 5 个逐字节不变，变的 2 个恰是 `z42c.semantics.zpkg` 本身。）
- [x] 1.13 `xtask test compiler`：自举不动点 gen1==gen2 3/3
- [x] 1.14 `xtask test` 全量 GREEN
- [x] 1.15 **`xtask test stdlib --mode jit` 两 shard**
- [x] 1.16 `xtask test bootstrap` 无越界

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

## 实施中的两处调整（与 DRAFT 的差异）

1. **helper 签名收 `Z42ClassType` 而非 `StrMap`**：`IrGenTypeEmitter` / `IrGenMemberEmitter`
   两处原本带 `owner == null ||` 守卫，收类型并在 helper 内部容忍 null，才能让 7 处真正统一
   （否则那两处仍要在调用点各写一次 null 判断）。
2. **刻意保留 2 处 `md.RegKey` 用法**（不属同一模板，改了会破坏字节中性）：
   - `DeclBinder._dupSigKey:124` —— 不是键解析，是拿 RegKey 取到符号后**重算全签名 MangleKey** 做判重
   - `TestIndexBuilder:63` —— 两档变体，回落发生在 **IR 名探测**层（`arityIr` → `bareIr`）而非符号表层；
     改用 helper 会丢掉 `Name$arity` 那次探测

## 明确不做

- **不改键的格式或规则**（primary 裸 / 非-primary 全签名不动）→ **无格式 bump、无两代自举**
- **不把 impl 方法并入 primary/非-primary 规则**
- **不动 `CallEmitter.z42:238-243`** 的静态 DepIndex 查找

## 铁律

- 「字节中性」是核心假设。**任何一次对账不通过就立刻停下重新分析**，不要「看起来差不多就推」。
- 派发相关改动的失败模式是**静默派发到错误目标**而非崩溃 → 必须跑 JIT 模式 + goldens，
  只看 interp 绿不算数。
- 分批提交，不图省事合并。
