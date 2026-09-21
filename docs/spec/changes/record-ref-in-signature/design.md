# Design: `ref` 随签名跨包传播

## Architecture

```
导出侧（本包编译）                              导入侧（下游包编译）
─────────────────────────                      ─────────────────────────
md.Params[j].IsRef
   ↓ ClassDescBuilder._paramAttrRefs
IrAttrRef("$ByRef", "")  ──→ IrFunction.ParamAttrs ──→ SIGS 段（已在 wire format 里）
                                                          ↓ TsigReconcile
                                                   ExportedParamZ.IsRef
                                                          ↓ ImportedSymbolLoader
                                                   Z42FuncType.ParamIsRef
                                                          ↓ HasRefInfo() 转真
                                                   RefArgCheck.CheckSymmetry 开始生效
```

---

## Decisions

### D1：骑 param attr-ref 通道，不 bump 格式

**可选路径与取舍：**

| | 做法 | 后果 |
|---|---|---|
| A | `ref` 编进 TSIG 的**类型串**（`"ref int"`，仿 `?` / `[]`） | 「已定义 section 字段语义变化」⇒ **必须 bump**（zbc + zpkg 各 4–5 步 + fixture 重生 + 本地死锁要靠 CI artifact overlay）。且旧读者会把 `"ref int"` 当未知类型名而失败 |
| B | `ExportedParamZ` 加一个 wire 字段 | 同样是格式变更 ⇒ 同样 bump |
| **C（选定）** | 逐形参 attr-ref 列表加 `$ByRef` 哨兵 | **零 bump、双向兼容** |

C 的兼容性论证：

- **新读者读旧包**：无 `$ByRef` ⇒ `IsRef=false` ⇒ `HasRefInfo()` 为真但全否。
  ⚠️ 这里有个坑：旧包的参数**确实可能是 ref**，判成全否会把跨包 `F(ref v)` 误报成多写。
  因此 `HasRefInfo()` 的判据**不能**只看数组长度（见 D2）。
- **旧读者读新包**：`$ByRef` 是不认识的哨兵 ⇒ 忽略。attr-ref 列表本就可扩展
  （`$Default` / `$Caller:` 同机制），旧读者只查自己认识的哨兵。

先例：`$Default`（PR6，注释自述「零格式-bump，骑 zbc 1.15 param attr-ref blob 通道」）
与 `$Caller:<kind>`（PR6b）。本变更与它们完全同构。

### D2：`HasRefInfo()` 的判据要换 —— 这是本变更最容易出错的一处

`simplify-ref-parameters` 里：

```
HasRefInfo() := ParamIsRef != null && ParamIsRef.Length == ParamCount
```

那时的语义是「本地签名填了 = 可信；导入签名空数组 = 不知道」。本变更让导入侧也填，
于是**长度判据不再能区分「已知全否」与「旧包，不知道」**：

| 来源 | ParamIsRef | 应当 |
|---|---|---|
| 本地签名 | 按 AST 填 | 可信 |
| 新包导入 | 按 `$ByRef` 填 | 可信 |
| **旧包导入**（无 `$ByRef` 通道的包） | 全 false | **不可信** —— 与「已知全否」长得一样 |

⇒ 加一个显式布尔 `RefInfoKnown`，由填充点置真；`HasRefInfo()` 读它，不再靠长度推断。
旧包的导入路径不置 ⇒ 保持跳过检查，行为与本变更前一致。

**这条不做就会产生假阳性**：用户升级编译器后，凡是引用了尚未重新编译的旧包并写了
`F(ref v)` 的地方，全部误报 E0473。

### D3：`this` 槽的偏移

`IrFunction.ParamAttrs` 按 **SIGS 参数序**对齐，实例方法含 `this` 槽（索引 0 为空列表）。
`TsigReconcile` 现有代码已用 `i + off` 处理这个偏移（`$Default` / `$Caller:` 同理），
`$ByRef` 沿用同一 `off`，不新增偏移逻辑。

### D4：不放宽 E0465

E0465（`ForwardNotRenderable`）拒绝跨包 `[Forward]` 的两个理由是「参数名丢了」+
「`ref` 判定不了」。前者已由 `fix-crosspkg-named-args` 修，后者由本变更修 ⇒ 两个前提都没了。

但**本变更不放宽它**：放宽要重新核对整条转发渲染路径（签名渲染、实参转发、`ref` 在生成代码里
的位置），属独立变更。本变更只在 E0465 的注释里记下「前提已解除」，免得后人以为还缺条件。

---

## Risks

| 风险 | 缓解 |
|---|---|
| **D2 的假阳性**（旧包被判成「已知全否」） | `RefInfoKnown` 显式布尔；跨包用例须覆盖「引用旧包」形态 |
| `ImportedSymbolLoader` 有多个签名构造点（#731 刚动过这片） | 逐个接线；漏掉的表现是该路径跨包不受检（静默退化，非误报）——用例要覆盖类方法 / 自由函数 / 接口方法 |
| 与 `free-function-overloads` 后续阶段文本冲突 | 按 workflow §2 不排队，rebase 时解 |
| 自举 | 元数据写侧变了 ⇒ 走冷种子；但**无格式 bump**，旧种子仍可读新包，不会死锁 |

**不涉及**：zbc / zpkg minor、fixture 重生、CI artifact overlay。
