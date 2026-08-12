# Proposal: 强制访问权限检查（private / protected / internal）

## Why

z42 编译器目前**完全不做访问权限检查**。`private` / `protected` / `internal` 被词法、语法解析并
存入符号表和 zpkg 元数据（`FieldSymbol.Visibility` / `MethodSymbol.Visibility`），但**没有任何一处
在绑定 / 类型检查时校验可见性**——诊断码 `E0404 AccessViolation` 预留却从未 emit。

后果（REPL 与普通编译同源，非 REPL 特有）：

```z42
class A { private int a; }
void Main() { A x = new A(); Console.WriteLine(x.a); }   // 类外读私有字段 → 零诊断编译通过
```

访问修饰符是语言已宣称支持的特性，parsed-but-not-enforced 属实现缺陷：封装形同虚设，用户误以为
`private` 生效。本变更从根因补上强制层，让预留的 `E0404` 真正生效。

## What Changes

- 新增访问权限强制：字段读/写、实例方法调用、静态成员访问、属性 getter/setter 在绑定期校验可见性，
  违规 emit `E0404`。
- 语义按 C#：
  - **private** — 仅声明类文本内可访问（含同类其它实例；派生类**不可**访问基类 private）。
  - **protected** — 声明类 + 派生类可访问（跨包派生同样允许）。
  - **internal** — 同包（同一 zpkg 编译单元）可访问；跨包不可。默认无修饰符成员即 internal。
  - **public** — 不校验。
- 为 `Z42ClassType` 增 `IsImported` 标志（import 加载期置位），供 `internal` 判定「声明类是否来自其它包」。
- **不改 zbc/zpkg 格式**（实现期实证）：成员可见性 `u8` 加值 3=internal 不改布局、reader 原样携带；只改
  `_visCode`（无修饰符→3）+ `_visStr`（3→"internal"）两个编码函数，跨包 internal 即生效。
- **override 继承基类可见性**：无显式修饰符的 `override` 视为 public（`_vis`/`_visCode`）——消除 ~99 处
  无修饰符 override（ToString/Stream/Dump）跨包地雷，正确 C# 语义。
- **record 定位字段公有**：`DeclParser` 合成 record 定位字段用 `public`（镜像 C# record）。
- **编译器 split 辅助类 45 处 `private→internal`**：z42c 的 parser 等同包协作辅助方法欠标注（无强制期
  遗留），改为 internal（同包互访的正确修饰符）；stdlib 私有零违规。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | NEW | 访问权限校验核心：`CheckAccess(vis, declClass, env, symbols, kind, name, span)` |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | 字段读 / getter / 方法组 / 实例方法调用 / 静态字段读 5 处接入 CheckAccess |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | 静态方法调用解析处接入 CheckAccess（若静态调用在此绑定） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | 属性赋值 `obj.Prop = x`（setter）路径接入 CheckAccess（若在此绑定） |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42ClassType` 增 `bool IsImported` 字段（默认 false）—— 已确认定义在此 |
| `src/compiler/z42c.semantics/src/IrGenFacts.z42` | MODIFY | `_visCode`：无修饰符→3(internal)、override→0(public) |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `_visStr`：3→"internal"（跨包 internal 恢复） |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `_vis`：override→public（继承基类可见性） |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | record 定位字段合成用 `public` mods |
| `src/compiler/z42c.syntax/src/{Parser,MemberParser,TypeParser,DeclParser}.z42` 等 | MODIFY | split 辅助类 45 处 `private→internal`（同包协作） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 构造 imported `Z42ClassType` 时置 `IsImported = true` |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件表补 AccessChecker |
| `docs/book/src/compiler/access-control.md` | NEW | 访问控制机制页（规则 + 强制点 + IsImported 策略 + 边界） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |
| `src/compiler/z42c.semantics/tests/access-control/access_control_tests.z42` (+toml) | NEW | 14 单元：private/protected/internal-同包/public/静态 + 违规 E0404 |
| `src/compiler/z42c.syntax/tests/decl/decl_tests.z42` | MODIFY | record-public AST golden ×3 更新 |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | vis=3 hex golden ×2 更新 |
| `src/tests/zbc-format/*/source.zbc` (6) | MODIFY | vis 字节 0→3 重生（committed 基线） |

> **跨包 internal 无自动化 e2e**：cross-zpkg harness 只做 build-and-compare（无 expected-compile-error
> 模式）。跨包 internal 的**逻辑**由 semantics 单元（CheckAccess + IsImported）覆盖，端到端由手工验证
> （field+method E0404 实证）；stdlib/toolchain 全量构建即海量跨包 public 回归门。不为此扩 harness（避免范围蔓延）。

**只读引用：**
- `src/compiler/z42c.semantics/src/TypeEnv.z42` — `CurrentClass()` 语义
- `src/compiler/z42c.semantics/src/Symbol.z42` — `FieldSymbol/MethodSymbol.Visibility / ContainingTypeName`
- `src/compiler/z42c.semantics/src/SymbolCollector.z42` — 默认可见性 `_vis`（无修饰符 = internal）
- `src/compiler/z42c.core/src/DiagnosticCodes.z42` — `E0404 AccessViolation`

## Out of Scope

- **友元 / `InternalsVisibleTo` 等可见性放宽机制** —— 不引入。
- **`protected internal` / `private protected` 组合级别** —— 当前 `_vis` 不产出组合级别，暂不支持。
- **反射绕过访问控制**（`FieldInfo.GetValue` 对 private）—— 反射是运行时能力，不在编译期强制范围。
- **修改任何现有成员的可见性以「修好」自身**（若 stdlib/自举出现跨包 internal 漏网）—— 那属实施期发现，
  按 workflow 停下汇报后单独处理，不在本 Scope 预先假设。

## Open Questions

- [ ] `internal` 强制后 stdlib / z42c 自举是否有跨包 internal 漏网访问？尽调显示纪律良好、预计极少；
      真实量以 `xtask test` 为准。若漏网**且量大**，触发 workflow 中断条件 → 停下汇报（可能建议标 public 或
      本变更收窄为 private+protected、internal 另立 change）。
