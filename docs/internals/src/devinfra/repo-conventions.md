# 本仓命名与目录约定

> 对齐：2026-09-17。面向**改 z42 本身**的人。用户代码的命名规则在参考手册的
> [命名约定](../../../reference/src/conventions/naming.md)——那是 SoT，本页只补
> "这个仓库自己额外遵守什么、以及它是怎么实现的"。

## 1. keyword ⟷ struct 别名的实现机制

用户看到的是"`int` 和 `Int32` 是同一个类型"。实现上归一在一张表里：

**`src/compiler/z42c.semantics/src/PrimModel.z42`**（change `unify-value-types` Phase 1）——
单一「关键字 ↔ `Std.*` 值类型」模型表。它接受**任意拼写**：

```
关键字 int  │ canonical 短名 i32 │ 包装名 Int32 │ FQ Std.Int32
        └──────────┴──────────────┴──────────┘
                    Canon() → "i32"
                       ↓
        由短名投影出 IrType / IrTag / 包装类名 / 反射名 …
```

设计不变式（byte-identical 门禁）：每个投影与它取代的历史函数**逐条同义**，故消费点切到
本表后 codegen 逐字节不变。

**归一覆盖**：整数族 `i8..u64`、`f32`/`f64`、`bool`、`char` 为 **Scalar 值类型**
（裸 `Value` 承载、算术热路径）；`string` / `object` 为**有专属 IR 标签的内建引用类型**
（非 Scalar，但 IrTag 保留 `Str`/`Ref` 以维持 byte-identical）。

> ⚠️ **`TypeRegistry.StdlibClassName` 已废弃。** 那是 C# bootstrap 编译器时代的入口，
> 在 z42 编译器里**零命中**。全仓唯一残留是 `src/runtime/src/metadata/well_known_names.rs:17`
> 的一句注释（"by the **C#** TypeChecker via `TypeRegistry.StdlibClassName`"）——
> 注释本身也已过期。看到旧文档提这个名字一律按 `PrimModel` 理解。

`PrimModel.Canon` 的实现细节（先按 `(长度, 首字符)` 分桶再比较，而不是 24 次顺序 `==`）是
热路径优化：绝大多数入参是用户类名，分桶后一次字符串比较都不做，最坏 3 次。

历史背景：primitive struct 改 BCL PascalCase 见
`docs/spec/archive/2026-05-24-rename-primitives-to-pascal-case/`（完整 12 条映射在那份
proposal 的「映射」节）；stdlib 违规命名的清理见
`docs/spec/archive/2026-05-24-fix-stdlib-naming-violations/`。

## 2. stdlib 包命名族

本仓的库包分两族（`src/libraries/` 下各一个目录，各带一份 `<pkg>.z42.toml`）：

| 族 | 含义 | 成员 |
|---|---|---|
| `z42.<topic>` | 标准库，随工具链分发，用户永不声明 | `z42.core` `z42.io` `z42.collections` `z42.text` `z42.numerics` `z42.json` `z42.toml` `z42.yaml` `z42.net` `z42.crypto` `z42.compression` `z42.encoding` `z42.regex` `z42.uri` `z42.random` `z42.threading` `z42.diagnostics` `z42.test` `z42.build` `z42.project` `z42.ir` `z42.cli` `z42.scripting` |
| `z42c.*` | 编译器自身的库 | `z42c.core` `z42c.syntax` |

> **没有 `z42.math` 包**——数学库叫 **`z42.numerics`**。旧文档里的 `z42.math` 是错的。

包名与命名空间**解耦**：`z42.collections` 包里是 `namespace Std.Collections`，
`z42.core` 里同时住着 `Std` / `Std.IO` / `Std.Collections` / `Std.Reflection` 多个命名空间。

**manifest 文件名**是 `<pkg>.z42.toml`（`z42.core.z42.toml`、`z42.numerics.z42.toml`），
不是裸 `z42.toml`；workspace 根是 `z42.workspace.toml`。
（`src/libraries/z42.toml/` 是**包名**叫 `z42.toml` 的 TOML 解析库，它的 manifest 叫
`z42.toml.z42.toml`——别把这个目录当成一份清单文件。）

## 3. `z42.core` 的实际目录树

```
src/libraries/z42.core/
├── z42.core.z42.toml
└── src/
    ├── Array.z42  Object.z42  String.z42  Math.z42  Convert.z42  …   → namespace Std
    ├── Collections/     → namespace Std.Collections
    ├── Delegates/       → namespace Std          ← 目录名 ≠ 命名空间段
    ├── Exceptions/      → namespace Std          ← 同上
    ├── Primitives/      → namespace Std          ← 同上
    ├── Protocols/       → namespace Std          ← 同上
    ├── GC/              → namespace Std
    ├── Native/          → namespace Std
    ├── Runtime/         → namespace Std.Runtime
    ├── Time/            → namespace Std
    ├── IO/              → namespace Std.IO
    └── Reflection/      → namespace Std.Reflection
```

**关键事实：目录名与命名空间段只是"常常"对应，不是规则。**
`Collections/` / `IO/` / `Reflection/` / `Runtime/` 确实映射到子命名空间；
**`Exceptions/` / `Delegates/` / `Primitives/` / `Protocols/` / `Time/` 全都是 `namespace Std;`**。
旧的 naming-conventions 文档里"`Exceptions/` → `namespace Std.Exceptions`"和
"`Delegates/` → `Std.Delegates`"是错的，同一份文档后面讲 Exception 时又写对了（自相矛盾）。

同理：stdlib 里**没有** `Std.Collections.Generic` 命名空间，只有 `Std.Collections`。

`_` 前缀的文件（`Time/_DateTimeHelpers.z42`）= 该目录的内部辅助，不是公开主类型。

## 4. `tests/` 目录

与 `src/` 平级的 `tests/` 放该库的自测。**文件名可用 snake_case**
（`tests/string_format.z42`）——自测不公开，不受 §11 文件名规则约束。
目录形态的多文件测试用 `tests/<name>/source.z42` 作入口。

同理 `src/tests/` 下的 VM golden 用例（`src/tests/delegates/event_keyword_multicast/` 等）
一律 snake_case 目录 + `source.z42` + `expected_output.txt`。

## 5. 位标志的 workaround：`ModeFlags`

z42 **没有** `[Flags]` attribute，enum 上也不支持 `|` 位运算。需要位标志的地方用
「class + `static int` 常量 + 手工位运算」：

```z42
// src/libraries/z42.core/src/Delegates/SubscriptionRefs.z42
public class ModeFlags {
    public static int None = 0;
    public static int Once = 1;
    public static int Weak = 2;
}
```

消费侧直接写字面量位掩码（`if ((modes & 2) != 0)`），**不引用常量**——这是为了避开跨成员
符号带来的冷启动 stale-cache 问题，与 `DiagnosticCodes` 里一批"语义层用字面量发码"是同一
手法。改这些数值时要连同消费点的字面量一起改。

`[Flags]` attribute 落地后，这类容器可以正式升级为 enum。

## 6. 线程局部存储：三个候选语法（均未落地）

z42 当前**没有**线程局部存储语法。async/await + 多线程正式引入时，候选形态：

```z42
// 候选 1：attribute
[ThreadLocal]
private static int _localCounter;

// 候选 2：modifier
private threadlocal int _localCounter;

// 候选 3：泛型容器
private static ThreadLocal<int> _localCounter = new ThreadLocal<int>();
```

**已确定的部分**：无论最终选 1/2/3，**命名规则与静态字段相同**
（`PascalCase` 公开 / `_camelCase` 私有），不加 `t_` 前缀；差异通过 attribute / modifier /
类型表达。落地时需回头校验这条承诺仍成立。

## 7. Deferred

### `Async` 后缀决策

L3 async/await 引入时确定 `Async` 后缀策略（C# 风格的 `LoadAsync()` 还是 Swift / Rust 的
无后缀）。当前 stdlib 无 async 代码，参考手册已写明"目前不要主动加 `Async` 后缀"。

### `z42-fmt` 自动 enforce

`z42-fmt` 落地时把命名约定转成 lint 规则。当前除 E0444 / E0445 / E0447 三条后缀强制外，
违反命名约定不会编译失败；`z42-fmt` 集成后可选 `--strict` 模式 enforce。

## 关联文档

- [命名约定](../../../reference/src/conventions/naming.md)（参考手册）—— 用户代码命名规则的 SoT
- [源代码编译流程](../compiler/source-compile.md) —— `PrimModel` 所在的 TypeCheck 阶段
- [错误码体系](../compiler/error-codes.md) —— E0444 / E0445 / E0447 的发射点与"怎么加一个码"
