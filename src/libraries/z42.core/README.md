# z42.core — 核心库

## 职责

所有 z42 程序隐式依赖的基础类型，对应 .NET `System` 命名空间核心部分。

## 如何测试验证

```bash
xtask test stdlib z42.core                      # 本库全部 [Test] 单元
xtask test stdlib z42.core -k string_methods    # 只跑一个单元
xtask test e2e --dir libraries/z42.core         # 本库的 Main-based golden 用例
```

`tests/` 下两种形态并存，选哪种由**断言要绑到哪个 `Assert`** 决定：

| 形态 | 例子 | 说明 |
|------|------|------|
| `tests/<name>.z42`（`[Test]` 单元） | `string_methods.z42` | 绝大多数库 API 行为都写成这个。文件里 `using Std.Test` → 裸名 `Assert` 是 **Std.Test.Assert**（抛 `TestFailure`）|
| `tests/<name>/source.z42`（Main golden） | `std_assert/`、`math/` | 只在**需要 prelude 那份 `Std.Assert`**、或需要 sidecar（`expected_output.txt` / `interp_only`）时用。`std_assert` 正是前者：它测的就是 `Std.Assert` 本身，写成 `[Test]` 会被 `using Std.Test` 抢走裸名，反而测不到 |

> String 的库行为（Length / ByteLength / Trim / Split / Join / Format / Object 协议…）
> 集中在 `tests/string_methods.z42` + `tests/string_bcl_augment.z42`。字符串**字面量语法**
> （raw string、插值、拼接）不在这里，归 [src/tests/strings/](../../tests/strings/)。

## src/ 核心文件

| 文件 | 内容 |
|------|------|
| `Object.z42` | 所有类型的基类，`ToString()`、`Equals()` 等协议方法 |
| `String.z42` | 字符串类型；最小 intrinsic 核（`Length` / `CharAt` / `FromChars` / `ToCharArray` / `Equals` / `CompareTo` / `GetHashCode` + bulk 原语 `Substring`→`__str_substring`、`ConcatParts`→`__str_concat_parts`（perf-stdlib-hot-paths；`Join` / `Concat` / `StringBuilder.ToString` 经它一次拼接）），其余方法（`Contains` / `StartsWith` / `EndsWith` / `IndexOf` / `Replace` / `ToLower` / `ToUpper` / `Trim*` / `IsNullOr*` / `Split` / `Format`）为纯脚本实现 |
| `SplitOptions.z42` | bitwise 标志位 `Std.SplitOptions.{None, RemoveEmptyEntries, TrimEntries}`（int 常量，OR 组合）— `String.Split(sep, options)` 参数 |
| `Int32.z42` | `struct int` — 整数基元（`Parse` / `TryParse`(→`int?`) / `CompareTo` / `Equals` / `GetHashCode` / `ToString` + INumber op_* 纯脚本实现；`MaxValue` / `MinValue` 常量）|
| `Int64.z42` | `struct long` — 64-bit 整数（同 Int32，含 `TryParse`→`long?` + `MaxValue` / `MinValue`）|
| `Double.z42` | `struct double` — 双精度浮点（同 Int32，含 `TryParse`→`double?` + IEEE-754 分类 `IsNaN` / `IsInfinity` / `IsPositiveInfinity` / `IsNegativeInfinity` / `IsFinite` + 常量 `MaxValue` / `MinValue` / `Epsilon` / `NaN` / `PositiveInfinity` / `NegativeInfinity`）|
| `Single.z42` | `struct float` — 单精度浮点（VM 用 F64 存储；常量 `MaxValue` / `MinValue` / `Epsilon`）|
| `SByte/Byte/Int16/UInt16/UInt32/UInt64.z42` | 其余整数基元家族（`Parse` / `TryParse` / 协议方法 + `MaxValue` / `MinValue` 常量，C# BCL 对标）|
| `Bool.z42` | `struct bool` — 布尔（只实现 `IEquatable<bool>`）|
| `Char.z42` | `struct char` — 字符（`CompareTo` / `Equals` / `GetHashCode` / `ToString` / `IsWhiteSpace` / `ToLower` / `ToUpper` + ASCII 分类 `IsDigit` / `IsLetter` / `IsLetterOrDigit` / `IsUpper` / `IsLower` / `IsPunctuation`；上述分类与 casing 均有 C# 风格静态形式 `Char.IsDigit(c)` 等）|
| `Type.z42` | 运行时类型对象（`typeof` 运算符返回值）|
| `Array.z42` | `T[]` 基类（`Length` / `Clone` / 反射 `CreateInstance` / `GetValue` / `SetValue`）+ 静态算法（C# `System.Array` 对标）：排序 `Sort<T>` / `Sort<T>(cmp)` / `Sort<T>(index,length)` / `Sort<TKey,TValue>(keys,items)`（配对排序）、查找 `IndexOf` / `IndexOf(start[,count])` / `LastIndexOf` / `LastIndexOf(start)` / `Contains` / `BinarySearch` / `BinarySearch(index,length,value)` / `BinarySearch(value,cmp)`、谓词 `Find` / `FindLast` / `FindIndex` / `FindLastIndex` / `FindAll` / `Exists` / `TrueForAll`、变换 `ConvertAll` / `ForEach` / `Copy` / `Copy(srcIdx,dstIdx,len)` / `Fill` / `Fill(value,start,count)` / `Reverse` / `Reverse(index,length)` / `Clear` / `Resize` / `Empty`、只读视图 `AsReadOnly<T>`（→ `Collections.ReadOnlyCollection<T>`） |
| `Assert.z42` | 断言工具（`Assert.True`、`Assert.Equal` 等）|
| `Convert.z42` | 类型转换工具：`ToInt32` / `ToInt64` / `ToDouble` / `ToString`（native）+ `ToByte` / `ToInt16` / `ToSingle` / `ToBoolean` / `ToChar`（纯脚本）|
| `Math.z42` | `static Math` — 常量 `Pi` / `E` / `Tau`；纯脚本 `Abs` / `Max` / `Min` / `Sign` / `Clamp`（+ `*Int` 变体）/ `Truncate` / `Round(x,digits)`；native (libm) `Pow` / `Sqrt` / `Floor` / `Ceiling` / `Round` / `Log` / `Log10` / `Log2` / `Exp` / `Sin` / `Cos` / `Tan` / `Asin` / `Acos` / `Atan` / `Atan2` / `Sinh` / `Cosh` / `Tanh` / `Cbrt`（C# `System.Math` 对标）|
| `IEquatable.z42` | 相等性接口 |
| `IComparable.z42` | 比较接口 |
| `IDisposable.z42` | 资源释放接口（`void Dispose()`）|
| `IEnumerable.z42` | 可迭代契约（`IEnumerator<T> GetEnumerator()`）|
| `IEnumerator.z42` | 前向迭代器契约（`bool MoveNext()` + `T Current { get; }`）|
| `IComparer.z42` | 双参数比较器契约（Wave 3）|
| `IEqualityComparer.z42` | 双参数相等性 + 哈希契约（Wave 3）|
| `IFormattable.z42` | 自定义格式化契约（Wave 3）|
| `INumber.z42` | 数值约束接口（`op_Add` / `op_Subtract` / `op_Multiply` / `op_Divide` / `op_Modulo`）|
| `Exception.z42` | 异常基类（`Message` / `StackTrace` / `InnerException`）|

## src/Exceptions/ — 标准异常子类（Wave 2 2026-04-25）

| 文件 | 继承自 | 语义 |
|------|--------|------|
| `ArgumentException.z42` | Exception | 参数非法 |
| `ArgumentNullException.z42` | ArgumentException | 参数为 null |
| `InvalidOperationException.z42` | Exception | 对象状态不允许此操作 |
| `NullReferenceException.z42` | Exception | 解引用 null |
| `IndexOutOfRangeException.z42` | Exception | 索引越界 |
| `KeyNotFoundException.z42` | Exception | 字典查找键不存在 |
| `FormatException.z42` | Exception | 字符串解析失败 |
| `NotImplementedException.z42` | Exception | 方法未实现 |
| `NotSupportedException.z42` | Exception | 方法不支持当前场景 |

详见 `docs/design/language/exceptions.md`。

## src/Collections/ — 基础泛型集合三件套

| 文件 | 内容 |
|------|------|
| `Collections/List.z42` | `List<T>` — 泛型动态数组核心（纯脚本实现）；`Sort()` 稳定归并排序 O(n log n)，`List(int capacity)` 预分配构造，`AddRange` 一次预扩容 |
| `Collections/List.Query.z42` | `List<T>` 查询/谓词族（partial 第二部分，对标 C#）：`Find` / `FindLast` / `FindIndex` / `FindLastIndex` / `FindAll` / `Exists` / `TrueForAll` / `RemoveAll` / `LastIndexOf` / `GetRange` / `BinarySearch`（`ConvertAll` / `AsReadOnly` 见 Deferred） |
| `Collections/Dictionary.z42` | `Dictionary<K,V>` — 泛型哈希映射（纯脚本实现）；`Remove` 复用已存 hash 内联重排探测链；查找族 `TryAdd` / `GetValueOrDefault(key)` / `GetValueOrDefault(key, default)`（`TryGetValue` 待 out→tuple 迁移后引入） |
| `Collections/ReadOnlyCollection.z42` | `ReadOnlyCollection<T>` — 只读集合视图（对标 C# `ObjectModel.ReadOnlyCollection`）；按引用包装 `T[]`，`Count` / 只读索引器 / `Contains` / `IndexOf` / `CopyTo` / `ToArray` / foreach；无变更 API。由 `Array.AsReadOnly<T>` 构造 |
| `Collections/HashSet.z42` | `HashSet<T>` — 泛型哈希集合（开放寻址，与 `Dictionary` 同款掩码探测 + 存储 hash 短路）；`Add`（去重，已存返 false）/ `Remove` / `Contains` / `Count` / `IsEmpty` / `Clear` / `ToArray` + 集合运算 `UnionWith` / `IntersectWith` / `ExceptWith`（接收 `T[]`）|

### 设计决策（2026-04-25 reorganize-stdlib-packages W1）

C# BCL 对齐：`List<T>` / `Dictionary<K,V>` 作为"最基础泛型集合"物理驻留
在 `z42.core` 包的 `Collections/` 子目录（与 core 基础类型共享隐式 prelude 包），
namespace 仍是 `Std.Collections`（逻辑上与 `Queue` / `Stack` 等次级集合同
namespace）。

包位置（物理） vs namespace（逻辑）解耦对齐：
- **包**：`z42.core`（隐式加载，无需用户声明依赖）
- **源码目录**：`z42.core/src/Collections/`（与 core 扁平层的基础类型分目录）
- **namespace**：`Std.Collections`（仍需用户写 `using Std.Collections;` 才能无限定访问）

类比 C# BCL：`System.Collections.Generic.List<T>` 物理在 `System.Private.CoreLib`
assembly，源码在 `System/Collections/Generic/List.cs`，namespace 独立分层。

> `sources.include` 默认是 `src/**/*.z42`（递归通配），无需修改 manifest

### List<T> 尺寸例外（2026-09-02 add-list-dict-lookup-family）

`List<T>` 拆成两个 partial 文件（`List.z42` 核心 + `List.Query.z42` 查询族）以保持
**单文件**可读；但按 [code-organization.md](../../../../.claude/rules/code-organization.md)
「类型拆多文件时累计计入」，`List<T>` **整体**已超过 200 行的类型尺寸软/硬限。

这是**有意的对标例外**：`List<T>` 镜像 C# `System.Collections.Generic.List<T>`，其
公开成员面（动态数组核心 + 查询/谓词族 + 排序）本就庞大，硬性拆成多个辅助类型只会
破坏「一个 `List` 就是 C# 那个 `List`」的对标直觉、给调用方增加认知负担。故此处以
partial 解决文件可读性，类型整体尺寸作为记录在案的例外接受。`Dictionary<K,V>` 仍在
200 行以内，无需例外。

### Deferred（本次未纳入的 C# 成员）

- **`List<T>.ConvertAll<TOut>(Func<T,TOut>)`**：返回 `List<TOut>`，而 `List<>` 的类型
  约束要求 `TOut` 也满足 `IEquatable<TOut> + IComparable<TOut>` —— 需在方法级泛型上
  传播该约束，留独立 follow-up。
- **`List<T>.AsReadOnly()`**：`ReadOnlyCollection<T>` 已存在，但 C# 语义是**活视图**
  （随原 List 变化），而本类内部为容量数组，直接包装需决定「快照 vs 活视图」，留决策。
- **`Dictionary<K,V>.TryGetValue(key, out value)`**：依赖 `out`，而 ref-borrow 程序的
  out→tuple 迁移正在进行；待迁移落地后以最终 idiom 引入。`GetValueOrDefault` 覆盖多数
  无异常查找场景。
- **`Dictionary<K,V>.ContainsValue(value)`**：`TValue` 无约束，值相等判定需走 Object
  协议装箱路径，语义/性能待评估。
> 即可自动拾取子目录源文件。

### primitive-as-struct 设计（L3-G4b 重构）

`int` / `long` / `double` / `float` / `bool` / `char` 以 **`struct <小写名>`** 形式
声明；`string` 仍保留 uppercase `class String`（规范化映射）。C# BCL 模式对齐：
声明层面 primitive = struct，运行时层面 VM 仍用 unboxed `Value::I64` / `Value::F64`。

INumber 的 `op_Add` 等通过**纯脚本 body** 实现（`return a + b;`，2026-04-24 迁移到
C# 11 static abstract 形式），编译后走 IR AddInstr 等指令 — **零新 VM builtin，
零 codegen 特化**，遵守 Script-First。

### String extern 预算（2026-04-25 simplify-string-stdlib）

C# BCL 对齐：`string.Length` / `string[i]` 相当于 `[Intrinsic]` property；其余
方法（`Contains` / `IndexOf` / `Trim` / `Substring` / `ToLower` / `Replace`）用
C# 代码循环字符实现。z42 照此：

| 保留 extern（11 个）| 说明 |
|-------|------|
| `Length` / `CharAt` / `FromChars` | 最小 intrinsic 核；循环基础 |
| `Equals` / `CompareTo` / `GetHashCode` / `ToString` | Object 协议 |
| `Split` / `Join` / `Concat` / `Format` | 分配 / 变参 / 格式串较复杂，保留 |

| 迁移到脚本（11 个）| 实现方式 |
|-------|---------|
| `IsEmpty` (property) | `Length == 0` |
| `Contains` / `StartsWith` / `EndsWith` / `IndexOf` | CharAt 循环 |
| `Substring` / `Replace` / `ToLower` / `ToUpper` | `char[]` + `FromChars` |
| `Trim` / `TrimStart` / `TrimEnd` | `IsWhiteSpace` 扫描 |
| `IsNullOrEmpty` / `IsNullOrWhiteSpace` (static) | null 检查 + 循环 |

**索引语义统一**：所有索引 / 长度 / 切片按 **Unicode scalar (char)** 计数；
UTF-8 byte 视图不对外暴露。ASCII 场景与旧行为完全等价。
**Casing 语义**：ASCII 规则（`'A'..'Z'` ↔ `'a'..'z'`），locale-sensitive
casing 延后到 L3 `CultureInfo`。
