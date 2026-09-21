# Proposal: `ref` 随签名跨包传播

## Why

`simplify-ref-parameters`（#728，已合）让调用点漏写 / 多写 `ref` 报 E0472 / E0473，
但**只在同一个包内生效**：

- `TypeNameResolver.TsigTypeName` 只处理数组 / nullable / 泛型实参，**不记录 `ref`**
- `ImportedSymbolLoader` 建 `Z42FuncType` 时没有 ref 信息，`ParamIsRef` 留空数组
- `Z42FuncType.HasRefInfo()` 因此为假 ⇒ 对称性检查整条跳过

这不是新发现：`DiagnosticCodes.z42` 的 **E0465**（`ForwardNotRenderable`）注释早已记载
「`ref`/`out` 在 TSIG 格式里**根本不记录**……实测调用点少写 ref 照样编译通过、修改丢失」，
并因此整类拒绝了跨包 `[Forward]`。

### 为什么现在必须做

它是 `enforce-value-type-non-null` 的**硬前置**。那个 change 要把
`Int32.TryParse` 从 `int?` 改成 `bool TryParse(string, ref int)` —— 一旦这个 API 导出，
用户包里漏写 `ref` 就会**静默丢掉解析结果**（调用方传值不传址 ⇒ 出口 copy-out 没有 lvalue
可写回 ⇒ 读到的是自己的零值）。等于把本要根治的缺口成建制地推给用户。

`simplify-ref-parameters` 的 tasks.md 因此写了一条硬约束：
**在本变更落地前，stdlib 不得导出任何 `ref` 形参的公开 API。**

## What Changes

### 骑 param attr-ref 通道，**零格式 bump**

逐形参的 attr-ref 列表（`IrFunction.ParamAttrs`，SIGS 段）**已经在 wire format 里**，
且已有两个哨兵在用它：

| 哨兵 | 用途 | 引入 |
|---|---|---|
| `$Default` | 参数默认值的 ConstBlob | PR6，注释自述「**零格式-bump，骑 zbc 1.15 param attr-ref blob 通道**」 |
| `$Caller:<kind>` | caller 宏种类 | PR6b |

本变更加第三个：**`$ByRef`**（`FactoryFunc` 空）。

- 新读者读旧包：没有 `$ByRef` ⇒ 判为非 ref ⇒ 退化成今天的行为（安全）
- 旧读者读新包：`$ByRef` 是不认识的哨兵 ⇒ **忽略**（attr-ref 列表本就是可扩展的）

⇒ **双向兼容，不需要 zbc / zpkg minor bump**，因此也不触发
`version-bumping.md` 的 9 步仪式与「本地必须靠 CI artifact overlay」的死锁配方。

> 起草这条线时我一度认定必须 bump（因为想把 `ref` 编进 TSIG 的**类型串**，那确实是
> 「已定义 section 字段语义变化」）。`ExportedParamZ` 只有 4 个字段、无标志位，看起来没路。
> 直到发现 `$Default` 的注释——**这个仓库早就有一条零 bump 的旁路，而且是同一类需求**。
> ⭐ 教训：下「必须 bump」的结论前，先找找有没有既存的哨兵 / 可扩展通道。

### 三处接线

1. **写**：`ClassDescBuilder._paramAttrRefs` 对 `md.Params[j].IsRef` 追加 `$ByRef` 哨兵
2. **读**：`TsigReconcile` 从 param attr 块读出 → `ExportedParamZ.IsRef`
3. **落地**：`ImportedSymbolLoader` 把它填进 `Z42FuncType.ParamIsRef`
   ⇒ `HasRefInfo()` 转真 ⇒ 跨包调用点开始受检

### 连带解除的限制

- **E0465（`ForwardNotRenderable`）的一半理由消失** —— 它拒绝跨包 `[Forward]` 的两个理由是
  「参数名丢了」+「`ref` 判定不了」。前者已由 `fix-crosspkg-named-args` 修，后者由本变更修。
  是否放宽 E0465 归独立变更（要重新核对整条渲染路径），本变更只在注释里记下前提已满足。
- **stdlib 从此可以导出 `ref` 形参 API** ⇒ 解除 `enforce-value-type-non-null` 的阻塞。

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/libraries/z42.ir/src/IrModule.z42` | `IrParamDefault` 增 `ByRefSentinel = "$ByRef"` + `IsByRef(attrs, count)` 查询 |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | `ExportedParamZ.IsRef`（内存 DTO 字段，不入 wire） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | 读 `$ByRef` → `p.IsRef` |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | `_paramAttrRefs` 对 ref 形参追加哨兵 |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | `ExportedParamZ.IsRef` → `Z42FuncType.ParamIsRef`（各签名构造点） |
| `src/compiler/z42c.semantics/src/RefArgCheck.z42` | 注释更新（跨包缺口已补） |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | E0465 注释：`ref` 判定不了这条前提已解除 |
| `src/tests/cross-zpkg/**` | 新增跨包 `ref` 用例（正面 + 漏写阴性） |
| `docs/reference/src/language/parameter-modifiers.md` | 删掉 ⚠️「跨包暂不强制」一节 |

## Out of Scope

- **放宽 E0465 / 跨包 `[Forward]`** —— 需重新核对整条转发渲染路径，独立变更
- **格式 bump** —— 本变更刻意避开（见上）
- `enforce-value-type-non-null` 的 stdlib 迁移 —— 本变更只解除它的阻塞

## Open Questions

无。技术路径与 `$Default` / `$Caller:` 完全同构，有两个已验证的先例。
