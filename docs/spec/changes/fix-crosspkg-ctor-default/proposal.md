# Proposal: 跨包构造器的默认值形参从来没被注入过

> **状态：IMPL** | 创建：2026-09-13 | 由 `fix-ctor-arity-skew`（PR #620）的**对照组 fixture** 查出

## Why

`new Sized(5)` 对跨包的 `Sized(int w, int h = 3)`，`H` 读到 **0** 而不是作者声明的 **3**。

`ConstructTyper._bindNew` 手写了一套实参适配，只有两条路：

1. `cms.HasDecl && cms.Decl != null` → `_adaptArgs`（命名实参重排 + 默认值填充）
2. `_packParamsTail`（`params` 尾包）

**imported ctor 没有 `Decl`**（`ClassExtractor._extractClass` 从 AST 抽签名，不带体），所以第 1 条
走不到；少给的形参位于是**根本没被填**，callee 的寄存器停在零值。

方法路径一直有这一支 —— `OverloadBinder._withDefaults` 里的 `_crossPkgDefault`（PR6 修「跨包默认值
塌零」时加的，从导入签名的 `$Default` ConstBlob 解码 → AST → 重绑）。**只有 ctor 漏了**，与
`fix-ctor-overload-by-arity-only`（同 arity 不同类型的 ctor 没做真决议）、ctor 的 `params` 尾包
（PR #616，「方法路径一直有 `_withParamsExpansion`，只有 ctor 漏了」）是同一族。

**为什么一直没人发现**：跨包默认值的既有用例 `param_default_cross_pkg` **只覆盖静态方法**，
跨包 `params` 的 `params_cross_pkg` 同样只覆盖静态方法。构造器这条路没有任何跨包用例。

## What Changes

`_bindNew` 在 `_packParamsTail` **之前**补上跨包默认值那一支，与 `_withDefaults` 同序
（`params` 尾参由 `_packParamsTail` 负责，故排除）：

```
cms 无 Decl 且有 Signature 且 ArgCount < Signature.ParamCount
  ⇒ 前 ArgCount 位照抄，其余逐位 _crossPkgDefault(Signature, i, env, Span)
```

复用**同一个** `_crossPkgDefault` —— 它已经处理 caller 宏注入（`$Caller:*`）与 `$Default`
ConstBlob 解码，无默认值时保守回落零值。不新增机制。

## 验证

| 门 | 内容 |
|----|------|
| `crosspkg_ctor_default`（新 cross-zpkg fixture） | 标量 / string / enum 默认值、连省两个尾参、以及「显式给全实参」的对照 |
| 退回对照 | 把这一支去掉重建编译器，该 fixture 必须 FAIL |
| 全量 GREEN | `xtask test` 全 stage 通过 |
