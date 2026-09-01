# fix-static-class-instance-members

> 状态：IMPL · GREEN 中 | 分类：fix（含语言语义强制，轻量规范）| 2026-09-01

## What / Why

**问题**：标准库的 `Std.String` 被误标为 `public static class String : IComparable<string>,
IEquatable<string>`——但它是 primitive `string` 的包装类，`s.Length` / `s.CharAt(i)` / `s.Contains(...)`
等全是**实例方法**，还实现了 `IComparable` / `IEquatable`（实例契约）。`static class` 与「含实例成员 /
实现接口」自相矛盾。

**根因**：早期 z42c 的 `StubCollector` 在建类 stub 时**只读** `sealed` / `abstract` / `struct` 修饰，
**完全忽略 class 上的 `static`**——所以这类矛盾声明能静默通过编译，`String` 的错标一直未被发现。

**修正**（同一原子变更）：
1. **stdlib**：`Std.String` 改 `static class` → `sealed class`（对齐 C# `System.String`；sealed 禁继承 +
   启用去虚化）。全代码库扫描确认 `String` 是**唯一** offender。
2. **编译器**：新增诊断 **E0451**，在 `SymbolCollector._passSealedEnforce`（与 sealed 强制同遍）强制
   「static 类只容纳静态成员」——含实例方法 / 字段 / 属性 / 构造器 / 索引器、声明基类、实现接口皆报错
   （对标 C# CS0708 / CS0710 / CS0713 / CS0714）。

## 设计要点

- **发码用字面量 `"E0451"`**（不引用 `DiagnosticCodes.StaticClassInstanceMember` 常量）——沿用 E0449 /
  E0450 既有手法，规避 core→semantics 新增跨成员符号在 F2 冷启动撞 stale-cache。常量仍登记进
  `DiagnosticCodes.z42` 作码表 SoT，待随 nightly 载入后可切回常量引用。
- **合法成员**：`static` 方法 / 字段、`const` 字段（隐含静态）、嵌套类型（不是实例成员）。
- 约束仅对 `c.Kind == "class"`（struct 无 static 变体）。

## 验证

- `xtask test compiler`：z42c [Test] 23 单元全过（含新增 7 个 E0451 collect 用例）；self-host 不动点
  3/3 gen1==gen2 逐字节复现；e2e 自编译执行。
- `xtask build stdlib`：25/25 库编译成功（sealed String 通过 E0451）。
- `xtask test stdlib`：全绿。

## 影响面

| 文件 | 改动 |
|------|------|
| `src/libraries/z42.core/src/String.z42` | `static class` → `sealed class` + 注释 |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | 新增 `StaticClassInstanceMember = "E0451"` |
| `src/compiler/z42c.semantics/src/InheritanceResolver.z42` | `_passSealedEnforce` 加 static-class 强制 + `_checkStaticClassMember` helper |
| `src/compiler/z42c.semantics/tests/collect/collect_tests.z42` | 7 个 E0451 用例 |
| `docs/book/src/language/static-classes.md` | 新增语言页 |
| `docs/book/src/SUMMARY.md` | 挂载新页 |
