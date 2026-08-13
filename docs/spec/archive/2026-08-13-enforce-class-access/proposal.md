# Proposal: 类级访问强制（同包 private/protected 嵌套类引用）

> **拆分说明（2026-08-13）**：本 change 原设计含 ①同包 private/protected 嵌套类 + ②跨包 internal 类（后者需
> 类可见性进 zbc/zpkg 元数据 = **格式 bump**）。因格式-bump 本地自举撞 **macOS 两代自举墙**（`escape-stack`
> memory），经 User 裁决**拆分**：本 change 只落 **①（无格式 bump，本地完整 GREEN）**；**②跨包 internal 类**
> 拆为 follow-up change `enforce-crosspkg-internal-class`（走 CI 两代自举验证）。下文 Scope 已收窄为 ①。

## Why

成员级访问强制已落地（enforce-access-control #180 + default-member-private #181）：字段/方法/属性的
`private`/`protected`/`internal` 在绑定期校验。但**类型本身的可见性从不校验**——类的访问修饰符被解析进
`ClassDecl.Mods`（`DeclParser._parseModifiers`）却从未存到 `Z42ClassType`，任何一处**引用**一个类型时
都不检查该类型是否可见。后果：

```z42
class LinkedList { class Node { int v; } }              // Node 默认 private（嵌套→最小封闭作用域）
void Main() { var n = new LinkedList.Node(); }          // 类外引用 private 嵌套类 → 零诊断编译通过 ✗

// 包 A
internal class Secret { }                               // 默认 internal（顶层→模块）
// 包 B
void Use() { var s = new Secret(); }                    // 跨包引用 internal 类 → 零诊断编译通过 ✗
```

这是 default-member-private #181 明确列出的 Out of Scope 后续 change（「类级访问强制：private 嵌套类 /
internal 类不可跨作用域*引用*」），也是[语言规范](../../../design/language/access-control.md)承诺却未实现的
最后一块——规范的 `LinkedList.Node` 例子今天仍能被外部引用。封装在类型层面形同虚设。本变更补上强制层。

## What Changes

- **类型引用点访问强制**：在绑定期解析出一个类类型（`new T`、`T x`、`(T)e`、`e is T`、`e as T`、
  `typeof(T)`、`T.staticMember`、`catch(T)`、泛型实参 `List<T>` 等）后，校验**从当前上下文引用该类型**
  是否允许；违规复用 `E0404 AccessViolation`（C# 亦用单一 CS0122 覆盖成员与类型不可访问）。
- **语义（镜像 C#，与成员级一致）**：
  - **private 嵌套类** `Outer+Inner` —— 仅 `Outer` 文本内（及 `Outer` 的更深嵌套）可引用；`CurrentClass()`
    等于 `Outer` 或以 `Outer+` 为前缀。
  - **protected 嵌套类** —— `Outer` + 派生自 `Outer` 的类可引用（沿基链上溯）。
  - **internal 类**（含无修饰符顶层类默认）—— **本 change 内同包恒放行**；**跨包强制拆为 follow-up**（需类
    可见性序列化，格式 bump）。`CheckTypeRef` 保留 internal 分支（`IsImported && Visibility=="internal"` →
    deny），但因本 change 不序列化类可见性、imported 类 `Visibility` 默认 `public`，该分支对 imported 暂不触发。
  - **public 类** —— 不校验。
- **类可见性上 `Z42ClassType`（内存态，不序列化）**：新增 `string Visibility` 字段。本地类由 `SymbolCollector`
  从 `ClassDecl.Mods` 按**位置默认**（嵌套→`private`、顶层→`internal`）设置。
- **破坏面（已尽调，实测≈0）**：z42c 编译器顶层类 235/235 全 `public`；stdlib 生产顶层类型 337/337 全
  `public`（唯一 34 处无修饰符全为 `tests/`·`bench/` 测试 fixture，各自文件内自引用、无嵌套类越界引用）。故
  无同包 private/protected 嵌套类越界引用。纯诊断、不改 codegen → 自举 `gen1==gen2` 逐字节保持，**无格式 bump**。

## Scope（允许改动的文件）

### 语义（强制层）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | MODIFY | 新增**静态** `CheckTypeRef(Z42Type resolved, string currentClass, SymbolTable symbols, DiagnosticBag diags, Span sp)`：类级 private/protected/internal 校验；含 `_nestedOuter`（剥 `+` 末段取外层类）+ 嵌套可访问 + 派生判定（静态版）。internal 分支保留但跨包不触发（imported 可见性默认 public，序列化拆 follow-up） |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | 新增 `_chkTypeRef(Z42Type t, TypeEnv env, Span sp)` 包装：`AccessChecker.CheckTypeRef(t, env.CurrentClass(), env.Symbols, _diags, sp)`；供 binder 体引用点调用 |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | 局部变量 / catch 类型引用点解析后调 `_tc._chkTypeRef` |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `new` / cast / `is` / `as` / `typeof` / `default(T)` 引用点解析后调 `_tc._chkTypeRef`（泛型实参由 `CheckTypeRef` 递归覆盖） |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42ClassType` 增 `string Visibility`（默认 `"public"`，构造后由 collector 覆写；**内存态不序列化**） |
| `src/compiler/z42c.semantics/src/IrGenFacts.z42` | MODIFY | 新增静态 `classVisCode(mods,isNested)` / `classVisStr(code)` / `classVis(mods,isNested)`（位置默认单一真相） |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | ① `_putClassStub` 用 `IrGenFacts.classVis` 设 `ct.Visibility`（位置默认）；② `_chkTypeRef` 助手 + `_fillClass` 字段/属性/索引器/基类·接口 + `_methodSymbol` 参/返回类型解析后调（声明签名位置强制，全覆盖 D2；emit 到 `this.Diags`） |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | `E0404 AccessViolation` 注释补「亦覆盖类型引用」（复用，不新增码） |

> **② 跨包 internal 类（含格式 bump zbc1.33/zpkg0.38）拆为 follow-up change `enforce-crosspkg-internal-class`**：
> 涉及 `IrModule`/`ClassDescBuilder`/`ZbcWriter`/`ZbcReader`/`ZbcFormat`/`ZpkgWriter`/`ExportedTypes`/
> `TsigReconcile`/`ImportedSymbolLoader` + Rust `zbc_reader.rs` + fixture 重生（zbc-format 6 / zpkg-format 4 /
> zbc hex golden）+ cross-zpkg e2e。本 change **不含**这些。完整代码已存于该 follow-up 的 design + patch。

### 测试 + 文档

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.semantics/tests/access-control/access_control_tests.z42` | MODIFY | 体引用类级单元：private/protected 嵌套类越界 new/var/is→E0404、外层类内引用 OK、public 嵌套 OK、派生类引用 protected 嵌套 OK、顶层 internal 同包 OK |
| `src/compiler/z42c.semantics/tests/collect/collect_tests.z42` | MODIFY | 声明签名位置类级单元（收集期 `collectDiags`/`hasCode`）：字段/参/返/基类为 private 嵌套→E0404、public/同包 internal OK、外层类自有字段 OK |
| `docs/design/language/access-control.md` | MODIFY | Status → 类级同包强制已实现；跨包 internal 列 Deferred（follow-up） |
| `docs/book/src/compiler/access-control.md` | MODIFY | 加「类级访问强制」节（引用点 + 嵌套 outer 判定 + 两相位；跨包 internal 列 Deferred） |

**只读引用：**
- `src/compiler/z42c.semantics/src/TypeEnv.z42` — `ResolveType` / `CurrentClass()` / 嵌套 `+` 链上溯语义
- `src/compiler/z42c.semantics/src/NestedFlatten.z42` — 嵌套类 `Outer+Inner` 扁平化命名
- `src/compiler/z42c.semantics/src/MemberResolver.z42` — 成员级 `CheckAccess` 调用模式（镜像）
- `src/compiler/z42c.syntax/src/DeclParser.z42` — `_parseModifiers`（类访问修饰符解析 + E0405 组合拒绝）
- `.claude/rules/version-bumping.md` — zbc/zpkg minor bump checklist（本 change 的格式步骤依据）

## Out of Scope

- **不一致可访问性检查**（public 方法签名 / 字段暴露一个 internal 类型，C# CS0050–53）—— 独立更复杂的检查，
  本 change 做「引用点能否命名该类型」（含声明签名位置能否命名，D2 全覆盖），但**不**做「暴露面是否把较低
  可见性类型泄漏给更高可见性的 API」这一独立判定。列 Deferred。
- **顶层类标 `private`/`protected` 的声明期拒绝**（C# 顶层类型只能 public/internal）—— 声明合法性检查，与
  「引用强制」正交，列 Deferred。
- **反射绕过 / `as`·`is` 运行时**—— 运行时能力，不在编译期强制范围（沿用成员级 Out of Scope）。
- **修改任何现有类的可见性以「修好」自身**—— 尽调显示破坏面≈0；若实施期出现漏网跨包 internal 类引用，
  按 workflow 停下汇报，不预先假设。

## Open Questions

- 无。覆盖面已裁决（2026-08-13 gate）：**v1 全覆盖**——绑定期体引用 + 收集期声明签名位置（字段/参数/返回/
  基类·接口）。见 design.md D2 / D2b。
