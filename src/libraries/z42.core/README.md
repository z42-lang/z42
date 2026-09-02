# z42.core — 核心库

## 职责

所有 z42 程序隐式依赖的基础类型，对应 .NET `System` 命名空间核心部分。

## src/ 核心文件

| 文件 | 内容 |
|------|------|
| `Object.z42` | 所有类型的基类，`ToString()`、`Equals()` 等协议方法 |
| `String.z42` | 字符串类型；最小 intrinsic 核（`Length` / `CharAt` / `FromChars` / `Equals` / `CompareTo` / `GetHashCode` / `ToString` / `Split(sep)` + `Split(sep, options)` / `Join` / `Concat` / `Format`），其余方法（`Contains` / `StartsWith` / `EndsWith` / `IndexOf` / `Replace` / `Substring` / `ToLower` / `ToUpper` / `Trim*` / `IsNullOr*`）为纯脚本实现 |
| `SplitOptions.z42` | bitwise 标志位 `Std.SplitOptions.{None, RemoveEmptyEntries, TrimEntries}`（int 常量，OR 组合）— `String.Split(sep, options)` 参数 |
| `Int32.z42` | `struct int` — 整数基元（`Parse` / `TryParse`(→`int?`) / `CompareTo` / `Equals` / `GetHashCode` / `ToString` + INumber op_* 纯脚本实现；`MaxValue` / `MinValue` 常量）|
| `Int64.z42` | `struct long` — 64-bit 整数（同 Int32，含 `TryParse`→`long?` + `MaxValue` / `MinValue`）|
| `Double.z42` | `struct double` — 双精度浮点（同 Int32，含 `TryParse`→`double?` + IEEE-754 分类 `IsNaN` / `IsInfinity` / `IsPositiveInfinity` / `IsNegativeInfinity` / `IsFinite` + 常量 `MaxValue` / `MinValue` / `Epsilon` / `NaN` / `PositiveInfinity` / `NegativeInfinity`）|
| `Single.z42` | `struct float` — 单精度浮点（VM 用 F64 存储；常量 `MaxValue` / `MinValue` / `Epsilon`）|
| `SByte/Byte/Int16/UInt16/UInt32/UInt64.z42` | 其余整数基元家族（`Parse` / `TryParse` / 协议方法 + `MaxValue` / `MinValue` 常量，C# BCL 对标）|
| `Bool.z42` | `struct bool` — 布尔（只实现 `IEquatable<bool>`）|
| `Char.z42` | `struct char` — 字符（`CompareTo` / `Equals` / `GetHashCode` / `ToString` / `IsWhiteSpace` / `ToLower` / `ToUpper` + ASCII 分类 `IsDigit` / `IsLetter` / `IsLetterOrDigit` / `IsUpper` / `IsLower` / `IsPunctuation`；上述分类与 casing 均有 C# 风格静态形式 `Char.IsDigit(c)` 等）|
| `Type.z42` | 运行时类型对象（`typeof` 运算符返回值）|
| `Array.z42` | `T[]` 基类（`Length` / `Clone` / 反射 `CreateInstance` / `GetValue` / `SetValue`）+ 静态算法（C# `System.Array` 对标）：排序 `Sort<T>` / `Sort<T>(cmp)` / `Sort<T>(index,length)`、查找 `IndexOf` / `IndexOf(start[,count])` / `LastIndexOf` / `LastIndexOf(start)` / `Contains` / `BinarySearch` / `BinarySearch(index,length,value)` / `BinarySearch(value,cmp)`、谓词 `Find` / `FindLast` / `FindIndex` / `FindLastIndex` / `FindAll` / `Exists` / `TrueForAll`、变换 `ConvertAll` / `ForEach` / `Copy` / `Copy(srcIdx,dstIdx,len)` / `Fill` / `Fill(value,start,count)` / `Reverse` / `Reverse(index,length)` / `Clear` / `Resize` / `Empty`、只读视图 `AsReadOnly<T>`（→ `Collections.ReadOnlyCollection<T>`） |
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
| `Collections/List.z42` | `List<T>` — 泛型动态数组（纯脚本实现）；`Sort()` 稳定归并排序 O(n log n)，`List(int capacity)` 预分配构造，`AddRange` 一次预扩容 |
| `Collections/Dictionary.z42` | `Dictionary<K,V>` — 泛型哈希映射（纯脚本实现）；`Remove` 复用已存 hash 内联重排探测链 |
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
