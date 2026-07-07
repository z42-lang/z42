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
