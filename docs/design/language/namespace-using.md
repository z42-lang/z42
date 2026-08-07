# Namespace 与 Using — Phase 1 规范

> **Status**: L1 ✅ ｜ namespace + using 导入 + strict-using-resolution（spec/archive/2026-04-28-strict-using-resolution）

## 概述

Phase 1 完全对齐 C# 9–12 的 file-scoped namespace 语法，目标是：
- 支持 `namespace Foo.Bar;` 声明文件所属命名空间
- 支持 `using Foo.Bar;` 导入命名空间
- 支持 `using Alias = Foo.Bar.MyClass;` 类型别名
- IrGen 按命名空间限定函数/类名称（qualified name）
- Driver 按命名空间解析入口函数

---

## 语法

```
namespace_decl ::= "namespace" dotted_name ";"
using_decl     ::= "using" ( alias "=" )? dotted_name ";"
dotted_name    ::= IDENT ( "." IDENT )*
alias          ::= IDENT
```

- 每个文件至多一条 `namespace` 声明，必须出现在所有 `using` 之前、所有顶层声明之前
- `using` 声明紧跟在 `namespace`（或文件开头）之后，所有顶层声明之前
- Phase 1 不支持 block-scoped namespace（`namespace Foo { ... }`）

---

## 单文件语义

### 命名空间声明

```z42
namespace Demo;

class Point { ... }
void Helper() { ... }
```

- `Point` 的 qualified name = `Demo.Point`
- `Helper` 的 qualified name = `Demo.Helper`
- IrModule.Namespace = `"Demo"`

### Using 导入

```z42
using Std.IO;
using MyCorp.Utils;
```

`using` 声明激活对应 namespace 提供的**包**：
- 隐式 prelude（`z42.core`）始终激活，无需 using
- 其他包（含 stdlib `z42.io` / `z42.collections` 等）必须显式 using
- 见下方 [strict-using-resolution](#strict-using-resolution-2026-04-28)

### Using 别名

```z42
using MyPoint = Demo.Point;
```

Phase 1 中别名同样仅被 parser 记录，不做别名替换（单文件模式下别名等同于原始类型）。

---

## IR 名称限定规则

| 声明 | 无 namespace | 有 `namespace Foo` |
|------|-------------|-------------------|
| 顶层函数 `void Bar()` | `Bar` | `Foo.Bar` |
| 类 `class Baz` | `Baz` | `Foo.Baz` |
| 类方法 `class Baz { void M() }` | `Baz.M` | `Foo.Baz.M` |
| 构造函数 `class Baz { Baz() }` | `Baz.Baz` | `Foo.Baz.Baz` |

IrModule.Namespace 存储 `"Foo"`（或 `"main"` 当无 namespace 时）。

---

## 入口函数解析

Driver 按以下顺序查找入口函数：

1. `{Namespace}.Main`（有 namespace 时优先）
2. `Main`（无限定名回退）
3. `{Namespace}.main`（小写 main）
4. `main`（无限定名小写回退）

若找不到以上任一函数，报错退出。

---

## 多文件 / 多包编译（生效语义）

PackageCompiler 把每个源文件作为一个 CU 处理：

1. **Phase 0 – Parse all**：所有 CU 解析到 AST，收集每个 cu.Usings
2. **Phase 1 – Pass-0 collect**：用激活包过滤后的 `ImportedSymbols` 收集
   每个 CU 的类/接口/函数 shape
3. **Phase 2 – TypeCheck + IrGen**：身体绑定；遇到未激活包的类型 → E0401
   "undefined symbol"

**激活包计算**：
- prelude 包 (`z42.core`) 总是激活
- 用户每条 `using <ns>;` 激活该 namespace 提供的所有包
- 同包内多 CU 通过 intraSymbols 互见，无需相互 using

---

## file-scoped usings（强制，2026-08-07 add-global-using）

> **历史**：此前 `using` 事实上**包级泄漏**——`allUsings` 跨文件聚合激活依赖包，一个文件
> `using Std.Text;` 就让整包所有文件都能用 `StringBuilder`。这与 C#/Rust/Python/Go/TS 全部的
> 文件级作用域相悖，是隐性 footgun（删兄弟的 using → 不相关文件神秘崩）。现改为**强制文件级**。

**规则**：每个源文件**实际用到的跨包依赖 namespace** 必须被本文件的 `using`
（∪ prelude `{Std, Std.Runtime}` ∪ 本文件 namespace）覆盖，否则报
`E0436: namespace X is used but not imported in this file; add using X;`。

- **实现**：`IrDump.BuildPackageCus` 每个 CU 编完后 `_enforceFileScope`——比对该 CU 的
  `UsedDepNs`（codegen 实际命中的依赖 ns，DEPS 用同一份）与本文件 usings。**只读 UsedDepNs +
  追加诊断，不改任何 emit 字节** → 自举字节不动点不破。
- **范围**：只管**跨包依赖**（`z42.ir` / `z42.text` 等）；同包跨-ns（workspace 兄弟成员、intraSymbols
  互见）不受此约束（那由 workspace 链接解析，非 using）。同包严格 file-scope 留 follow-up。

### global using（逃生舱）

`global using X;`（`global` 是**上下文 token**，非新关键字——仅 CU 顶层紧跟 `using` 时识别）
**包级生效**：注入到包内每个 CU 的 using 集，满足其 file-scope 检查。团队 prelude / 真正处处要的
ns 一条 `global using` 搞定，file-scope 默认 + global using 可选 = C# 10 的成熟模型。

```z42
// prelude.z42（一处声明）
global using Std.IO;
global using Std.Collections;
// 包内其它文件无需再 using 即可用 Console / List<T>
```

- 实现：`UsingDecl.IsGlobal`（Parser 顶层 `global`+`using` 识别）；
  `IrDump._injectGlobalUsings` 收集全包 global usings 注入每个 CU 的 `Decls`（既有 per-CU using
  提取点 + `_enforceFileScope` 自动纳入）。
- 引入见 change `add-global-using`（`docs/spec/changes/` 或归档）。

---

## strict-using-resolution (2026-04-28)

**核心规则**：

1. `z42.core` 是唯一隐式 prelude（硬编码，扩展需 spec proposal）
2. 其他所有包（含 stdlib `z42.io` / `z42.collections` / `z42.text` /
   `z42.math` / `z42.test`）必须显式 `using <namespace>;` 才能激活
3. `using X;` 激活所有声明了 namespace `X` 中类型的非 prelude 包
4. 同 `(namespace, class-name)` 跨多激活包 → 编译错误 E0601
5. `using <ns>;` 无任何已加载包提供 → 编译错误 E0602
6. 非 stdlib 包（不以 `z42.` 开头）声明 `Std` / `Std.*` namespace →
   软警告 W0603（不阻断构建）

**典型场景**：

| 用户代码 | 行为 |
|---------|------|
| `new Object()` (无 using) | OK — Object 在 z42.core (prelude) |
| `new List<int>()` (无 using) | OK — List 在 z42.core 的 Std.Collections |
| `Console.WriteLine(...)` (无 using) | E0401 — Console 在 z42.io 未激活 |
| `Console.WriteLine(...)` + `using Std.IO;` | OK |
| `new Queue<int>()` (无 using) | E0401 — Queue 在 z42.collections 未激活 |
| `new Queue<int>()` + `using Std.Collections;` | OK |
| `using NoSuch.Pkg;` | E0602 |
| `using System;` | E0602（z42 没有 System namespace） |

**实现接入点**：

- `Z42.Core.PreludePackages` — prelude 名单 + reserved-prefix 检测
- `Z42.Pipeline.TsigCache.LoadForPackages(activated)` — 按包过滤加载
- `Z42.Semantics.TypeCheck.ImportedSymbolLoader.Load(modules, packageOf,
  activated, prelude)` — 主 API，输出包含 `PackageOf` + `Collisions`
- `Z42.Semantics.TypeCheck.TypeChecker.EmitImportDiagnostics` — 报 E0601/E0602
- `Z42.Pipeline.PackageCompiler.LoadExternalImported(tsigCache, userUsings, ...)` —
  生产路径
- `Z42.Pipeline.SingleFileCompiler.LocateImportedSymbols(path, userUsings)` —
  单文件路径

**补 using**：z42c 的 strict-using 报错会精确点名缺失的 `using`，按提示手动补一行即可。
（旧 `xtask audit` 正则启发式自动补齐工具已移除 —— redesign-xtask-test，2026-07-07。）

---

## 错误处理

| 错误情形 | 错误消息 |
|----------|----------|
| `namespace` 出现在顶层声明之后 | `namespace declaration must appear before any top-level declarations` |
| 同一文件出现两条 `namespace` | `duplicate namespace declaration` |
| `using` 出现在顶层声明之后 | `using directive must appear before any top-level declarations` |

---

## 示例

### 单文件带命名空间

```z42
namespace Demo;

class Point {
    int X;
    int Y;
    Point(int x, int y) {
        this.X = x;
        this.Y = y;
    }
    string ToString() {
        return $"({this.X}, {this.Y})";
    }
}

void Main() {
    var p = new Point(3, 4);
    Console.WriteLine(p.ToString());
}
```

生成的 IR 函数名：`Demo.Point.Point`、`Demo.Point.ToString`、`Demo.Main`

入口函数：`Demo.Main`

### Using 别名（记录但暂不解析）

```z42
namespace App;
using Std.IO;
using Pt = Demo.Point;

void Main() {
    Console.WriteLine("hello");
}
```

生成入口：`App.Main`
