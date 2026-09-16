# 构造器继承与隐式 base()

> 对齐日期：2026-09-15 · change `add-implicit-base-ctor-call`。语言规则见 [实例构造器与初始化子句](../../../book/src/language/constructors.md)。

两件事分在编译的两个阶段做：**收集期**给没写构造器的类合成构造器声明（继承来的 / 默认的）；**绑定期**给每个
实例构造器接上基类构造器调用（写了子句按子句、没写就是隐式 `base()`）。

```mermaid
flowchart LR
  subgraph 收集 SymbolCollector.CollectAll
    A[各 CU _passMembers<br/>注册显式 ctor] --> B[CtorInheritance.Run<br/>基类先于派生]
    B --> C[_passFixupOverrides …]
  end
  subgraph 绑定 DeclBinder
    D[_bindMethodBody] --> E{初始化子句}
    E -- this --> F[call this-ctor]
    E -- base / 隐式 --> G[BindCtorArgs 选目标 + 适配实参]
    G --> H[call base-ctor]
    D --> I[_injectFieldInits<br/>非 this 委托时]
  end
  B -. 合成的 MethodDecl 追加进 ClassDecl.Members .-> D
```

## 收集期：`CtorInheritance`

跑在**所有** CU 的成员收集之后（基类可能在别的文件），在 override 对齐等基链 pass 之前。对每个 class / struct：

```text
process(C):
  if 已处理 C: return;  标记已处理                    // 防继承环重入
  if C 是 static 类 / 导入类: return
  if C 已有任何实例 ctor: return                      // 写了构造器：不继承、不合成
  if C 是 class 且有非 Object 基类 B:
      if B 是本包类: process(B)                        // 基类先定案
  inh = B 的实例 ctor 中非 private 的，primary（裸键）在前、其余按注册键排序
  if inh 非空:
      for m in inh: 合成 `C(形参…) : base(形参…) { }`，第一个取裸键，其余全签名 mangle
  else if C（任一 partial 碎片）有实例字段 / auto 属性初始化器:
      合成 `public C() { }`
```

合成的是**普通的 `MethodDecl`**（`IsSynthesized = true`），追加到类的成员表并经 `SymbolCollector.RegisterMethod`
注册。之后的一切——重载决议、E0426、`where T : new()`、字段初始化器注入、隐式 `base()`、IR 发射、TSIG
导出——都走显式构造器那条路，没有第二套逻辑。

**形参类型用 `ResolvedTypeExpr`**：它直接携带基类构造器签名里**已解析的** `Z42Type`（泛型基类按
`GenericParamNames` 用 `MethodTypeArgSubst.ByName` 代换成派生类声明里写的类型实参），`SymbolTable.ResolveTypeP`
遇到它直接返回。不回写成源码拼写，原因是导入基类只有签名，而 TSIG 里的类型拼写对限定名是有损的。

默认值与 `params`：本地基类直接复用基类声明里的 `Param`（默认值表达式、`params`、`ref`）；导入基类从签名还原
——`$Default` ConstBlob 用 `ConstBlobReader` 解码成表达式，caller 宏还原成与 parser 同形的
`IdentExpr("$macro:<name>")`，`ParamsFrom` 还原 `params`。

**确定性**：继承来的构造器谁拿裸键（primary）决定字节，所以候选按「基类 primary 在前、其余按注册键」排序，
不依赖 `StrMap` 的槽位顺序（[跨语言共同陷阱 §1](../../../agent/rules/common-pitfalls.md)）。

## 绑定期：初始化调用

`DeclBinder._bindMethodBody` 对每个实例构造器：

| 情形 | 目标 | 行为 |
|---|---|---|
| `: this(args)` | 本类 | 调用；不注入本类初始化器（由被委托者注入） |
| `: base(args)` | 基类 | 调用 |
| 没写子句，基类无实例 ctor | — | 不调用 |
| 没写子句，基类有可零实参调用的 ctor | 基类 | 调用 `base()`（默认值 / 空 `params` 由实参适配补齐） |
| 没写子句，基类有 ctor 但无一可零实参调用 | — | E0469 |

「选哪个构造器 + 实参怎么适配」与 `new C(args)` 共用 `ConstructTyper._bindCtorArgs`（按类型的重载决议、
命名实参、本地 / 跨包默认值、`params` 打包）。此前初始化子句只按实参个数取键，是另一套口径。

合成体为空，最终体 = 本类初始化器 → 基类构造器调用 → （空）；执行顺序与 C# 一致。

## 删掉了什么

旧实现在绑定之后按**当前编译单元**的类表收集祖先链、把整条链的字段初始化器内联进一个 IR 期才发射的合成
构造器（`DeclBinder._synthCtors` + `IrGenTypeEmitter._emitSynthCtor`）。它不是符号、不进 TSIG，祖先链又只看
当前 CU——同包跨文件、跨包的基类初始化器因此静默丢失，派生类也无从调用基类的合成构造器。现在基类初始化器由
基类自己的构造器执行，两段代码已删除。

## 已知限制

- 「既写自己的构造器、又保留继承来的」（C++ `using Base::Base;`）不支持。
- 跨包 skew：编译时基类没有某个构造器、运行时的新版本有——派生类不会自动得到它，与所有编译期决议同类。
