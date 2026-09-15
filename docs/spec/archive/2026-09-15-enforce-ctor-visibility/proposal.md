# Proposal: 构造器可见性真正生效

> 变更类型：`lang`（完整流程）｜ 创建：2026-09-15 ｜ 来源：「推进 ctor 静默 bug」④ 实施中发现 ｜ User 裁决：默认 private（C# 规则）

## Why

文档（`docs/book/src/compiler/access-control.md`、`docs/design/language/access-control.md`）规定类的**字段 / 方法 / 构造器**
没写修饰符时默认 `private`，但编译器**从不检查构造器的可见性**：

```z42
class C { private C(int a) { } private int G() { return 2; } }
class U { void M() { var c = new C(1); c.G(); } }
// 只报 c.G() 的 E0404；new C(1) 调用 private 构造器照常编译
```

`AccessChecker.CheckAccess` 覆盖了字段、方法、属性，`ConstructTyper`（`new`）与构造器初始化子句（`: base` / `: this`）
都没有调用它。后果：
- 「私有构造器 + 静态工厂」这类封装写法形同虚设；
- ④（构造器继承）无法按可见性决定继承哪些构造器——大多数基类构造器没写修饰符，按文档是 private。

## What Changes

1. `new C(args)`（含 target-typed `new()`、对象初始化器、元组脱糖）与 `: base(args)` / `: this(args)` 在选中构造器后做可见性检查，
   违规报 **E0404**（与字段 / 方法同码、同判据：private 仅本类；protected 本类与派生类；internal 同包）。
2. **主构造器**（`class P(int X)`、`[Record] struct ValueTuple2<T1, T2>(…)`）的可见性为 **public**（对齐 C#）。
   parser 合成主构造器时修饰符由 `""` 改为 `"public"`。
3. 没写修饰符的普通构造器保持文档规定的 **private**。仓库内在类外构造的这类构造器补 `public`。

### 影响面普查（2026-09-15，临时编译器补丁 + 全量 `xtask test` + xtask 脚本，去重 407 处调用非 public 构造器）

| 情形 | 处数 |
|---|---|
| 没写修饰符的构造器在类外 `new` / 被派生类 `: base` 调用 | 377（70 个类） |
| 　其中是**主构造器**（第 2 条后自动合法） | 104（31 个类：含 stdlib `KeyValuePair`、`ValueTuple2..8`、`ListEnumerator`、z42.build `CompileRequest/CompileResult`、builder `Dirs/Inputs/Target` 等全部生产类型） |
| 　其中是**显式构造器**、须补 `public` | 273（39 个类：`src/tests` 为主；非测试仅 `examples/target_typed_new.z42`、`examples/closure_capture.z42`、`scripts/test/xtask_test_lib.z42`） |
| 本类内部调用非 public 构造器（合法） | 30 |
| 显式 `private` / `protected` / `internal` 被越权调用 | **0** |

另：编译器单测里以源码字符串构造的类（`SemanticDump` 用例）若在类外 `new` 无修饰符构造器，诊断计数会变化，按失败逐条补 `public`。

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/compiler/z42c.semantics/src/ConstructTyper.z42` | MODIFY | 选中构造器后 `CheckAccess` |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 初始化子句目标构造器 `CheckAccess` |
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | MODIFY | 头注释订正（默认 private）；`kind = "constructor"` 文案 |
| `src/libraries/z42c.syntax/src/DeclParser.z42` | MODIFY | 主构造器修饰符 `"public"` |
| `src/tests/**`、`examples/**`、`scripts/test/xtask_test_lib.z42`、`src/compiler/**/tests/**` | MODIFY | 类外构造的无修饰符构造器补 `public` |
| 新增 `src/tests/classes/ctor_visibility.z42`、typecheck 单测、cross-zpkg `ctor_visibility_cross_pkg/` | NEW | 回归 |
| `docs/book/src/compiler/access-control.md`、`docs/book/src/language/constructors.md` | MODIFY | 构造器检查点 + 主构造器 public |

## Out of Scope

- `where T : new()` 是否要求 **public** 无参构造器（C# 要求）——未普查，另立。
- 运行期反射创建实例（`Activator` 类）与泛型 `new T()` 的可见性。
- ④ `add-implicit-base-ctor-call`（本变更合并后 rebase 继续，继承时按可见性过滤即成立）。

## Open Questions

- 无（默认可见性已裁决；主构造器 public 对齐 C#，见 design D2）。
