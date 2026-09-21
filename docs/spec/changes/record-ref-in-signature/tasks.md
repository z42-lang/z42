# Tasks: `ref` 随签名跨包传播

> 状态：🟡 进行中 | 创建：2026-09-22
> 分支/worktree：`record-ref-in-signature` @ `wt-refnull`（基于 origin/main #732）
> 类型：`lang`（元数据语义，**无格式 bump** —— 骑 param attr-ref 通道）
> 授权：User「持续实现，PR 变绿自动合并」= 阶段 6.5 批量授权

## 进度概览
- [ ] 阶段 1: 哨兵 + 写侧
- [ ] 阶段 2: 读侧 + 落进签名
- [ ] 阶段 3: `RefInfoKnown` 换判据（防旧包假阳性）
- [ ] 阶段 4: 跨包用例
- [ ] 阶段 5: 文档 + GREEN + PR

## 阶段 1: 哨兵 + 写侧
- [ ] 1.1 `IrModule.z42` —— `IrParamDefault.ByRefSentinel = "$ByRef"` + `IsByRef(attrs, count)`
- [ ] 1.2 `ClassDescBuilder._paramAttrRefs` —— `md.Params[j].IsRef` 为真时追加哨兵（与 `$Default` 同手法）
- [ ] 1.3 确认三个调用点都走到（`IrGenMemberEmitter:39` 实例/静态、`:59` abstract stub、`IrGenAuxEmitter:111` 自由函数）

## 阶段 2: 读侧 + 落进签名
- [ ] 2.1 `ExportedTypes.z42` —— `ExportedParamZ.IsRef`（内存 DTO，不入 wire）
- [ ] 2.2 `TsigReconcile` —— 读 `$ByRef` → `p.IsRef`，沿用现有 `i + off`（this 槽偏移）
- [ ] 2.3 `ImportedSymbolLoader` —— 逐个签名构造点把 `IsRef` 填进 `Z42FuncType.ParamIsRef`
      （⚠️ #731 刚动过这片；漏掉的表现是该路径跨包不受检，静默退化非误报）

## 阶段 3: 换判据（本变更最易错的一处）
- [ ] 3.1 `Z42Type.z42` —— `Z42FuncType.RefInfoKnown` 布尔；`HasRefInfo()` 改读它，不再按长度推断
- [ ] 3.2 `SymbolCollector._methodSymbol`（本地签名）置真
- [ ] 3.3 `ImportedSymbolLoader`：**仅当该包带 `$ByRef` 通道时**置真
      —— 判据待定，见 3.4
- [ ] 3.4 完备性标记 `$RefSig` 放 `ParamAttrs` **槽 0**（判据选型与否决理由见 design §D2）
- [ ] 3.5 阴性用例：引用旧包写 `F(ref v)` 不得报 E0473

## 阶段 4: 跨包用例
- [ ] 4.1 `src/tests/cross-zpkg/ref_crosspkg/`：漏写 E0472、正确通过、多写 E0473
- [ ] 4.2 覆盖实例方法（this 槽偏移）、自由函数、接口方法
- [ ] 4.3 旧包形态用例（阶段 3.5）

## 阶段 5: 文档 + GREEN
- [ ] 5.1 `reference/language/parameter-modifiers.md` —— 删 ⚠️「跨包暂不强制」一节
- [ ] 5.2 `DiagnosticCodes.z42` E0465 注释 —— 记下「`ref` 判定不了」这条前提已解除（但不放宽）
- [ ] 5.3 `simplify-ref-parameters/tasks.md` 的硬约束标为已解除
- [ ] 5.4 `xtask test all` + `cargo test --lib` + examples + docs 全绿
- [ ] 5.5 **`CacheStore.CompilerFingerprint++`** —— 本变更改了编出的 zpkg 字节但不 bump 格式，
      按 version-bumping.md「编译器语义指纹」必须累加；CI 的 `guard-compiler-fingerprint` 会守门
- [ ] 5.6 PR + auto-merge
