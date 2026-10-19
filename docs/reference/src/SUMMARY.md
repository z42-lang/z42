# Summary

[前言](README.md)

---

# 语言

- [概览](language/README.md)

# 语言 · 基础

- [源文件的顶层结构](language/syntax.md)
- [doc 注释](language/doc-comments.md)
- [基本类型与字面量](language/types.md)
- [类型转换](language/conversions.md)
- [运算符](language/operators.md)
- [控制流](language/control-flow.md)
- [字符串](language/strings.md)
- [数组](language/arrays.md)
- [元组](language/tuples.md)
- [集合字面量](language/collection-literals.md)

# 语言 · 函数

- [函数与方法](language/functions.md)
- [参数修饰符（`ref` / `params`）](language/parameter-modifiers.md)
- [命名实参](language/named-arguments.md)
- [委托与事件](language/delegates-events.md)
- [闭包与捕获](language/closures.md)
- [`methodof`（方法引用表达式）](language/methodof.md)

# 语言 · 类型定义

- [类](language/classes.md)
- [继承与多态](language/inheritance.md)
- [结构体](language/structs.md)
- [接口](language/interfaces.md)
- [enum（枚举）](language/enums.md)
- [`[Record]` 与主构造器](language/record-attribute.md)
- [嵌套类型](language/nested-types.md)
- [partial 类型（跨文件类型定义）](language/partial-types.md)
- [sealed 修饰符](language/sealed.md)
- [static 类（只容纳静态成员）](language/static-classes.md)

# 语言 · 成员

- [属性与索引器](language/properties-indexers.md)
- [readonly 字段](language/readonly-fields.md)
- [const 编译期常量](language/const.md)
- [实例构造器与初始化子句](language/constructors.md)
- [静态构造函数](language/static-constructors.md)
- [包级初始化（`[ModuleInit]`）](language/module-initializers.md)
- [静态成员的名字解析](language/static-members.md)
- [`[Forward]`（成员转发）](language/member-forwarding.md)
- [对象初始化器](language/object-initializers.md)
- [target-typed new（省略构造类名）](language/target-typed-new.md)

# 语言 · 泛型

- [泛型约束（`where` 子句）](language/generic-constraints.md)
- [泛型方法（方法级类型参数）](language/generic-methods.md)

# 语言 · 语义

- [所有权与内存模型](language/memory-model.md)
- [迭代（foreach）](language/iteration.md)
- [`using` 语句](language/using-statement.md)
- [模式匹配](language/pattern-matching.md)
- [异常](language/exceptions.md)

# 语言 · 组织与可见性

- [Namespace 与 Using](language/namespaces.md)
- [访问权限控制](language/access-control.md)

# 语言 · 元数据

- [特性（Attributes）](language/attributes.md)
- [`available!()` 符号可用性探测](language/available-macro.md)

# 标准库

- [概览](stdlib/README.md)

# 标准库 · 核心（`z42.core`，隐式加载）

<!-- z42-doc:summary:core-begin（xtask doc gen 维护，勿手改） -->
- [类型总览](stdlib/core.md)
    - [Std]()
        - [Array](stdlib/core.Array.md)
        - [Assert](stdlib/core.Assert.md)
        - [Attribute](stdlib/core.Attribute.md)
        - [BitConverter](stdlib/core.BitConverter.md)
        - [Convert](stdlib/core.Convert.md)
        - [DelegateOps](stdlib/core.DelegateOps.md)
        - [Action](stdlib/core.Action.md)
        - [Func](stdlib/core.Func.md)
        - [Predicate](stdlib/core.Predicate.md)
        - [ISubscription](stdlib/core.ISubscription.md)
        - [MulticastAction](stdlib/core.MulticastAction.md)
        - [MulticastSubscription](stdlib/core.MulticastSubscription.md)
        - [MulticastFunc](stdlib/core.MulticastFunc.md)
        - [MulticastFuncSubscription](stdlib/core.MulticastFuncSubscription.md)
        - [MulticastPredicate](stdlib/core.MulticastPredicate.md)
        - [MulticastPredicateSubscription](stdlib/core.MulticastPredicateSubscription.md)
        - [ModeFlags](stdlib/core.ModeFlags.md)
        - [StrongRef](stdlib/core.StrongRef.md)
        - [OnceRef](stdlib/core.OnceRef.md)
        - [CompositeRef](stdlib/core.CompositeRef.md)
        - [WeakRef](stdlib/core.WeakRef.md)
        - [Disposable](stdlib/core.Disposable.md)
        - [Enum](stdlib/core.Enum.md)
        - [AggregateException](stdlib/core.AggregateException.md)
        - [ArgumentException](stdlib/core.ArgumentException.md)
        - [ArgumentNullException](stdlib/core.ArgumentNullException.md)
        - [ArgumentOutOfRangeException](stdlib/core.ArgumentOutOfRangeException.md)
        - [ArithmeticException](stdlib/core.ArithmeticException.md)
        - [ArrayTypeMismatchException](stdlib/core.ArrayTypeMismatchException.md)
        - [DivideByZeroException](stdlib/core.DivideByZeroException.md)
        - [Exception](stdlib/core.Exception.md)
        - [FormatException](stdlib/core.FormatException.md)
        - [IndexOutOfRangeException](stdlib/core.IndexOutOfRangeException.md)
        - [InvalidCastException](stdlib/core.InvalidCastException.md)
        - [InvalidMarshalException](stdlib/core.InvalidMarshalException.md)
        - [InvalidOperationException](stdlib/core.InvalidOperationException.md)
        - [KeyNotFoundException](stdlib/core.KeyNotFoundException.md)
        - [MissingSymbolException](stdlib/core.MissingSymbolException.md)
        - [MulticastException](stdlib/core.MulticastException.md)
        - [NotImplementedException](stdlib/core.NotImplementedException.md)
        - [NotSupportedException](stdlib/core.NotSupportedException.md)
        - [NullReferenceException](stdlib/core.NullReferenceException.md)
        - [ObjectDisposedException](stdlib/core.ObjectDisposedException.md)
        - [OutOfMemoryException](stdlib/core.OutOfMemoryException.md)
        - [OverflowException](stdlib/core.OverflowException.md)
        - [PlatformNotSupportedException](stdlib/core.PlatformNotSupportedException.md)
        - [SwitchExpressionException](stdlib/core.SwitchExpressionException.md)
        - [TimeoutException](stdlib/core.TimeoutException.md)
        - [TypeInitializationException](stdlib/core.TypeInitializationException.md)
        - [UnauthorizedAccessException](stdlib/core.UnauthorizedAccessException.md)
        - [TestFailure](stdlib/core.TestFailure.md)
        - [SkipSignal](stdlib/core.SkipSignal.md)
        - [ForwardAttribute](stdlib/core.ForwardAttribute.md)
        - [GC](stdlib/core.GC.md)
        - [GCHandleType](stdlib/core.GCHandleType.md)
        - [GCHandle](stdlib/core.GCHandle.md)
        - [HeapStats](stdlib/core.HeapStats.md)
        - [SoftHandle](stdlib/core.SoftHandle.md)
        - [WeakHandle](stdlib/core.WeakHandle.md)
        - [Guid](stdlib/core.Guid.md)
        - [Lazy](stdlib/core.Lazy.md)
        - [Math](stdlib/core.Math.md)
        - [Object](stdlib/core.Object.md)
        - [OperatingSystem](stdlib/core.OperatingSystem.md)
        - [Platform](stdlib/core.Platform.md)
        - [OSKind](stdlib/core.OSKind.md)
        - [ArchKind](stdlib/core.ArchKind.md)
        - [Boolean](stdlib/core.Boolean.md)
        - [Byte](stdlib/core.Byte.md)
        - [Char](stdlib/core.Char.md)
        - [Double](stdlib/core.Double.md)
        - [Int16](stdlib/core.Int16.md)
        - [Int32](stdlib/core.Int32.md)
        - [Int64](stdlib/core.Int64.md)
        - [SByte](stdlib/core.SByte.md)
        - [Single](stdlib/core.Single.md)
        - [UInt16](stdlib/core.UInt16.md)
        - [UInt32](stdlib/core.UInt32.md)
        - [UInt64](stdlib/core.UInt64.md)
        - [IBasicCollection](stdlib/core.IBasicCollection.md)
        - [IComparable](stdlib/core.IComparable.md)
        - [IComparer](stdlib/core.IComparer.md)
        - [IDisposable](stdlib/core.IDisposable.md)
        - [IEnumerable](stdlib/core.IEnumerable.md)
        - [IEnumerator](stdlib/core.IEnumerator.md)
        - [IEqualityComparer](stdlib/core.IEqualityComparer.md)
        - [IEquatable](stdlib/core.IEquatable.md)
        - [IFormattable](stdlib/core.IFormattable.md)
        - [INumber](stdlib/core.INumber.md)
        - [SplitOptions](stdlib/core.SplitOptions.md)
        - [String](stdlib/core.String.md)
        - [Type](stdlib/core.Type.md)
        - [TypeVisibility](stdlib/core.TypeVisibility.md)
        - [ValueTuple2](stdlib/core.ValueTuple2.md)
        - [ValueTuple3](stdlib/core.ValueTuple3.md)
        - [ValueTuple4](stdlib/core.ValueTuple4.md)
        - [ValueTuple5](stdlib/core.ValueTuple5.md)
        - [ValueTuple6](stdlib/core.ValueTuple6.md)
        - [ValueTuple7](stdlib/core.ValueTuple7.md)
        - [ValueTuple8](stdlib/core.ValueTuple8.md)
        - [Version](stdlib/core.Version.md)
    - [Std.Runtime]()
        - [Clock](stdlib/core.Clock.md)
        - [Entropy](stdlib/core.Entropy.md)
        - [AppProperties](stdlib/core.AppProperties.md)
        - [AssemblyLoadContext](stdlib/core.AssemblyLoadContext.md)
        - [ModuleLoader](stdlib/core.ModuleLoader.md)
        - [ModuleSearch](stdlib/core.ModuleSearch.md)
        - [RuntimeConfig](stdlib/core.RuntimeConfig.md)
        - [Runtime](stdlib/core.Runtime.md)
    - [Std.Collections]()
        - [Dictionary](stdlib/core.Dictionary.md)
        - [DictionaryEnumerator](stdlib/core.DictionaryEnumerator.md)
        - [HashSet](stdlib/core.HashSet.md)
        - [KeyValuePair](stdlib/core.KeyValuePair.md)
        - [List](stdlib/core.List.md)
        - [ListEnumerator](stdlib/core.ListEnumerator.md)
        - [ReadOnlyCollection](stdlib/core.ReadOnlyCollection.md)
    - [Std.IO]()
        - [DirectoryNotFoundException](stdlib/core.DirectoryNotFoundException.md)
        - [FileNotFoundException](stdlib/core.FileNotFoundException.md)
        - [IOException](stdlib/core.IOException.md)
        - [PathTooLongException](stdlib/core.PathTooLongException.md)
        - [Console](stdlib/core.Console.md)
        - [ConsoleError](stdlib/core.ConsoleError.md)
        - [Directory](stdlib/core.Directory.md)
        - [Environment](stdlib/core.Environment.md)
        - [File](stdlib/core.File.md)
        - [FileStreamNative](stdlib/core.FileStreamNative.md)
        - [Path](stdlib/core.Path.md)
        - [ProcessNative](stdlib/core.ProcessNative.md)
        - [ChildProcessNative](stdlib/core.ChildProcessNative.md)
        - [ProcessHandleNative](stdlib/core.ProcessHandleNative.md)
    - [Std.Threading]()
        - [LockRecursionException](stdlib/core.LockRecursionException.md)
        - [SynchronizationLockException](stdlib/core.SynchronizationLockException.md)
        - [ThreadNative](stdlib/core.ThreadNative.md)
        - [MonitorNative](stdlib/core.MonitorNative.md)
    - [Std.Net.Sockets]()
        - [TcpSocketNative](stdlib/core.TcpSocketNative.md)
        - [UdpSocketNative](stdlib/core.UdpSocketNative.md)
        - [TlsSocketNative](stdlib/core.TlsSocketNative.md)
        - [DnsNative](stdlib/core.DnsNative.md)
        - [NetTcpNative](stdlib/core.NetTcpNative.md)
        - [NetTlsNative](stdlib/core.NetTlsNative.md)
        - [UdpNative](stdlib/core.UdpNative.md)
    - [Std.Reflection]()
        - [Activator](stdlib/core.Activator.md)
        - [Assembly](stdlib/core.Assembly.md)
        - [ConstructorInfo](stdlib/core.ConstructorInfo.md)
        - [FieldInfo](stdlib/core.FieldInfo.md)
        - [MemberInfo](stdlib/core.MemberInfo.md)
        - [MethodBase](stdlib/core.MethodBase.md)
        - [MethodInfo](stdlib/core.MethodInfo.md)
        - [ParameterInfo](stdlib/core.ParameterInfo.md)
        - [PropertyInfo](stdlib/core.PropertyInfo.md)
    - [Std.Time]()
        - [DateTime](stdlib/core.DateTime.md)
        - [DateTimeOffset](stdlib/core.DateTimeOffset.md)
        - [Stopwatch](stdlib/core.Stopwatch.md)
        - [TimeSpan](stdlib/core.TimeSpan.md)
        - [TimeZone](stdlib/core.TimeZone.md)
        - [_DateTimeHelpers](stdlib/core._DateTimeHelpers.md)
<!-- z42-doc:summary:core-end -->

# 标准库 · 数据与文本

- [JSON](stdlib/json.md)
- [TOML](stdlib/toml.md)
- [YAML](stdlib/yaml.md)
- [正则](stdlib/regex.md)
- [文本处理](stdlib/text.md)
- [编解码](stdlib/encoding.md)
- [URI](stdlib/uri.md)
- [次级集合](stdlib/collections.md)

# 标准库 · 系统与 IO

- [流式 I/O 与子进程](stdlib/io.md)
- [网络](stdlib/net.md)
- [并发](stdlib/threading.md)
- [压缩](stdlib/compression.md)
- [日志与运行时自省](stdlib/diagnostics.md)

# 标准库 · 应用支撑

- [命令行参数](stdlib/cli.md)
- [随机数](stdlib/random.md)
- [密码学](stdlib/crypto.md)
- [大数与数值类型](stdlib/numerics.md)
- [测试与基准](stdlib/harness.md)

# 工具链

- [概览](toolchain/README.md)
- [`z42` 命令参考](toolchain/cli-z42.md)
- [`z42c` / `z42b` / `z42d` 命令参考](toolchain/cli-z42c-z42b.md)
- [工程清单 z42.toml](toolchain/z42-toml.md)
- [插件（Analyzer / Generator / Hook）](toolchain/compile-time-extensions.md)
- [运行时设置（旋钮清单）](toolchain/runtime-settings.md)

# 嵌入

- [概览](embedding/README.md)
- [C ABI 契约](embedding/c-abi.md)
- [原生互操作（native interop）](embedding/native-interop.md)

# 测试

- [编写与运行测试](testing.md)

# 约定

- [命名约定](conventions/naming.md)

---

# 附录

- [概览](appendix/README.md)
- [错误码全量表](appendix/error-codes.md)
