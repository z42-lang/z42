# z42.core — 核心库

## 职责

所有 z42 程序隐式依赖的基础类型（隐式 prelude 包，VM 启动时无条件加载，用户项目不声明依赖）。不含平台能力库（io / net / threading 等）。

## 功能索引

源码按子目录分组，逐项职责见 [src/README.md](src/README.md)；GC 控制与句柄见 [src/GC/README.md](src/GC/README.md)。

| 能力 | 入口 |
|------|------|
| Object / String / 数值 primitive（`struct int` 等） | `src/Object.z42` / `src/String*.z42` / `src/Primitives/` |
| 基础泛型集合 `List<T>` / `Dictionary<K,V>` / `HashSet<T>` / `ReadOnlyCollection<T>`（namespace `Std.Collections`） | `src/Collections/` |
| 接口契约（`IEquatable` / `IComparable` / `IEnumerable` / `INumber` 等） | `src/Protocols/` |
| 标准异常族 | `src/Exceptions/` |
| 断言 `Assert`（全仓唯一，失败抛 `TestFailure`） | `src/Assert.z42` + `src/Failure.z42` |
| 时间 `DateTime` / `TimeSpan` / `Stopwatch` | `src/Time/` |
| 委托 / 多播 / 订阅 | `src/Delegates/` |
| 反射 / 动态加载 / 运行时配置 | `src/Reflection/` / `src/Runtime/` |
| 控制台 / 文件 / 路径 / 环境（native 语义层） | `src/IO/` / `src/Native/` |

## 如何测试验证

```bash
xtask test stdlib z42.core                      # 本库全部 [Test] 单元
xtask test stdlib z42.core -k string_methods    # 只跑一个单元
xtask test e2e --file std_assert                # 本库的 Main-based golden 用例（按名 / 路径子串）
```

`tests/` 下两种形态并存，选哪种由**要不要 sidecar 文件**决定
：

| 形态 | 例子 | 说明 |
|------|------|------|
| `tests/<name>.z42`（`[Test]` 单元） | `string_methods.z42` | 绝大多数库 API 行为都写成这个。裸名 `Assert` = 唯一那份 `Std.Assert`（命名空间 `Std`、打包在 z42.test，失败抛 `TestFailure`）|
| `tests/<name>/source.z42`（Main golden） | `std_assert/`、`math/` | 需要 sidecar（`expected_output.txt` / `interp_only`）时用。`std_assert` 顺带是「零 `using` 也能拿到 `Assert`」的守卫（`src/tests/` 下大量同款 golden 依赖这条）|

> String 的库行为（Length / ByteLength / Trim / Split / Join / Format / Object 协议…）
> 集中在 `tests/string_methods.z42` + `tests/string_bcl_augment.z42`。字符串**字面量语法**
> （raw string、插值、拼接）不在这里，归 [src/tests/strings/](../../tests/strings/)。

## 设计要点

- **集合的包位置与 namespace 解耦**：`List<T>` / `Dictionary<K,V>` / `HashSet<T>` 物理在 `z42.core/src/Collections/`（随 prelude 隐式加载），namespace 仍是 `Std.Collections`，用户写 `using Std.Collections;` 才能无限定访问；`Queue` / `Stack` 等次级集合在 [z42.collections](../z42.collections/README.md)。
- **`List<T>` 尺寸例外**：拆为 `List.z42`（核心）+ `List.Query.z42`（查询族）两个 partial 文件保文件可读；类型整体超 [code-organization.md](../../../docs/agent/rules/code-organization.md) 的类型尺寸限，属有意的记录在案的例外（公开成员面本就庞大）。`Dictionary<K,V>` 在限内。
- **primitive 是 struct**：`int` / `long` / `double` / `float` / `bool` / `char` 等以 `struct <小写名>` 声明，运行时仍是 unboxed `Value::I64` / `Value::F64`；`string` 为 `class String`。`INumber` 的 `op_*` 为纯脚本 static abstract 实现，零 VM builtin（Script-First）。
- **String 只保留最小 extern 核**（`Length` / `ByteLength` / `CharAt` / `FromChars` / `Equals` / `CompareTo` / `GetHashCode` / `ToCharArray` / `Substring` / `ConcatParts`），其余方法为纯脚本；`Split` / `Join` / `Concat` / `Format` 在 `String.Split.z42`，`Insert` / `Remove` / `Pad*` 在 `String.Edit.z42`。
- **索引与 casing**：索引 / 长度 / 切片按 Unicode scalar（char）计数，UTF-8 byte 视图不对外暴露；`ToLower` / `ToUpper` 为 ASCII 规则，locale-sensitive 待 `CultureInfo`。

## 依赖关系
无依赖（隐式 prelude）；native 能力经 `src/` 内唯一声明点（`Clock` / `BitConverter` / `Entropy` / `Native/` / `IO/` 等）暴露给上层包。

## 待办
- `List<T>.ConvertAll<TOut>`：`List<>` 的类型约束要求 `TOut` 也满足 `IEquatable + IComparable`，需在方法级泛型上传播约束
- `List<T>.AsReadOnly()`：需决定快照 vs 活视图语义（`Array.AsReadOnly<T>` 已提供）
- `Dictionary<K,V>.TryGetValue(key, out value)`：待 out→tuple 惯用法落地；现用 `TryAdd` / `GetValueOrDefault` 覆盖
- `Dictionary<K,V>.ContainsValue(value)`：`TValue` 无约束，值相等需走 Object 协议，语义/性能待评估
