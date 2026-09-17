# 语言特性的决策台账

> 对齐：2026-09-17 ｜ 代码：`src/libraries/z42c.syntax/src/Lexer.z42`（关键字表）、
> `src/compiler/z42c.semantics/src/HandlerRegistry.z42`（内建 attribute 集）
>
> 规则面（怎么写、报什么错、有哪些形态）**一律在**[语言参考](../../reference/src/language/README.md)，
> 本页不重复。这里只回答一个问题：**为什么是这个形状，而不是另一个。**

改 z42 时要提一个语言特性提案，先在这里找同类决策——多数争论上一轮已经吵过一次。
顶层取舍（为谁设计、五条不可动摇的约束）见[设计哲学](philosophy.md)。

---

## 怎么读这一页

每条 = **决策**（一句话）+ **为什么**（排除了什么）+ **规则在哪**。
「还没有的东西」单列一节，它是当前状态的一部分，不是遗漏。

---

## 一、类型与值

### 两套等价拼写，短名是规范形式

C# 关键字（`int` / `double`）与 Rust 短名（`i32` / `f64`）指向同一个类型。
编译器把每种拼写**归一到短名**再比较，因此它们不能用来区分重载。

**为什么**：C# 拼写让迁移者零成本上手，短名让位宽一眼可见。选一个牺牲另一个都不划算；
归一到短名而不是关键字，是因为位宽才是类型的本质，关键字只是别名。

→ [基本类型与字面量](../../reference/src/language/types.md)

### `string` 内部是 UTF-8，`char` 是完整 Unicode 标量值

**为什么不是 UTF-16**：VM 里一个字符串对象就是「长度 + 内联 UTF-8 字节」的单次分配，
C ABI、文件与网络 I/O 也全按 UTF-8 走——选 UTF-16 等于每次跨边界都转一次码。
代价是按索引取字符不再是 O(1)，由 `CharAt` 走 builtin 承担；长度则两种口径都给
（`Length` 按 Unicode 标量、`ByteLength` 按字节且 O(1)）。

`char` 选 32 位（而不是 C# 的 16 位 UTF-16 code unit），是为了让「一个 `char` = 一个字符」
这条直觉成立，代理对问题在语言层直接消失。

→ [字符串](../../reference/src/language/strings.md)

### struct 是值类型，`[Record]` 是正交的第三根轴

`class` / `struct` 是**身份轴**（引用 vs 值）；`[Record]` 是 opt-in 的**数据载体轴**，
两种身份都能戴。带 `[Record]` 的类型获得位置参数展开为 public 字段 + 主构造器 + 值相等 +
记录式 `ToString` + 反射里的 `IsRecord` 标记。

**为什么不是 `record` 关键字**：C# 的 `record` 既能是 class 又能是 struct，把一根正交的轴
缠进了身份轴。用 attribute 表达同一件事，作用于两种身份，语言机制更简单，也不占关键字预算——
`record` 在 z42 里是普通标识符，可以当类名和变量名（实测可用）。

→ [`[Record]` 与主构造器](../../reference/src/language/record-attribute.md)、
[结构体](../../reference/src/language/structs.md)

### 元组由 `ValueTupleN` 承载，arity 2–8

`(a, b)` 在语义层 desugar 成 `new Std.ValueTupleN<...>`，背后是 stdlib 里真实的 `[Record] struct`。

**为什么有上界**：8 之后应该定义一个具名类型。不设上界要么递归嵌套（`ValueTuple8<...,TRest>`
那套 C# 把戏），要么无限生成——两者都是为极少数用例付永久复杂度。

→ [元组](../../reference/src/language/tuples.md)

## 二、空值

### `T?` 是注解，不是类型

`?` 在类型解析时被擦除：`T?` 和 `T` 解析成同一个类型。没有 `Nullable<T>`，没有可空性流分析。
`??` 与 `?.` 是真实的运算符，有 codegen。

**为什么不做 `Option<T>`**：`Option<T>` 要完整发挥价值，需要原生 ADT + 穷尽 `match` + 模式绑定
同时到位；只做一半会得到一个又啰嗦又没有安全保证的类型。宁可先把 `?` 留作**意图标注**，
等三件套齐了再一次性做对。

⚠️ 这条的副作用要写在脸上：**z42 当前没有空安全。** `string s = null;` 编得过，
空引用在运行期解引用时才暴露。

→ [基本类型与字面量 · 可空标记](../../reference/src/language/types.md)

## 三、错误处理

### 只有异常

`try` / `catch` / `finally` / `throw`，自定义异常靠继承。没有 `Result<T, E>`，没有 `?` 传播。

**为什么**：异常对 L1/L2 的语义足够，且与 C# 心智一致。`Result` 的价值在于
「错误路径显式且零开销」，但它要好用就必须有 `?` 传播 + ADT + 穷尽 match；
在那套东西到位前引入 `Result`，只会得到一个手写 `switch` 的噩梦。两者将来**共存**，不互相取代。

`catch { }` 能捕获非对象抛出物，`StackTrace` 只在 interp 路径自动填充（JIT 路径留空）——
这两条是实现现状，不是设计意图。

→ [异常](../../reference/src/language/exceptions.md)

## 四、抽象与多态

### 单继承 + 任意多接口 + Rust 式 `impl` 块

一个类只有一个基类；接口可以实现任意多个；跨包给已有类型补实现走
`impl Trait for Type` 块，静态解析成自由函数。

**为什么是 `impl` 而不是 `trait`**：被 `impl` 的那个东西就是 `interface`，不需要第二种抽象类型。
`impl` 块解决的是「我想给别人的类型加实现，但改不了它的源码」——这是 C# 扩展方法解决不了的
（扩展方法满足不了接口），而 Rust 的 orphan 规则形态刚好合适。

### 没有多继承，所以有 `[Forward]`

单继承是[不可动摇的约束](philosophy.md)之一，组合是唯一替代。但组合出来的链式访问累赘，
于是 `[Forward]` 把一个字段的部分成员面**生成为外层类型上的真方法**。

**为什么生成真方法而不是编译期重写调用点**：只有真方法才能满足接口、被反射看见、被 IDE 跳转。

→ [接口](../../reference/src/language/interfaces.md)、
[`[Forward]` 成员转发](../../reference/src/language/member-forwarding.md)

## 五、函数与闭包

### 顶层函数是一等的

不需要套一个 class。表达式体 `=>`、默认参数值（调用点展开）、`params` 变长参数都在。

**为什么**：脚本与小程序不该为「写一个函数」交类的税。这条也是
[单一命令入口](#单一命令入口)的前置——单文件程序用顶层 `void Main()` 就能跑，
`z42 run <file>.z42` 直接编译并执行。

### 闭包按值快照捕获值类型

创建闭包时**拷贝**被捕获值类型变量的当前值；引用类型捕获的是对象身份，内外共享同一个对象。

**为什么与 C# 反着来**：C# 把捕获变量提升进 display class 按引用共享，代价是两类经典陷阱
——循环变量晚绑定、值类型幻读。z42 选快照，一次性消掉两者。代价是「闭包里改外层变量」写不了，
需要共享可变状态时请显式共享一个对象。

→ [函数与方法](../../reference/src/language/functions.md)、
[闭包与捕获语义](../../reference/src/language/closures.md)、
[命名实参](../../reference/src/language/named-arguments.md)

## 六、泛型

### 代码共享 + 运行期具化，两头都不选

类级与方法级类型参数、Rust 风格 `where` bounds（`+` 组合、关联类型）、约束检查、类型实参推断都在。
实现策略是 **C# 式代码共享 + 具化**：一份字节码服务所有实例化，TypeDesc 携带 type_args，
`typeof(T)` / `is T` / `as T` 在运行期都可用。

**为什么不单态化（Rust / C++）**：VM 的 `Value` 已经是统一表示，执行时照样按 tag 派发——
单态化**消不掉**这层开销，却要付代码膨胀和编译变慢的账。同理，**值类型特化也不做**：
`Value` 把数值统一成 I64 / F64，特化收益极小。

**为什么不 Java 式擦除**：擦除丢掉运行期类型信息，`List<int>` 与 `List<string>` 不可区分，
`typeof(T)` / `is` / `as` 全部作废。反射是 z42 的一等能力，这条不能让。

> ⚠️ 源码注释里把这叫「类型擦除」（`EmitContext` / `ClassDescBuilder`），指的是
> **方法体内看不到具体 class name**，不是运行期没有类型实参。两者别混。

### 类型实参推断只驱动诊断，不回写调用点

调用点省略 `<...>` 时，编译器按结构统一形参类型与实参类型推出方法级类型实参，
用于实参检查与 `where` 约束校验——但**不把结果写回发出的调用指令**。

**为什么不回写**：回写会把 opcode 从 `Op.Call` 翻成 `Op.CallGeneric`、打乱 zbc 字符串池、
并让解释器的 native 快路径失效；而树里绝大多数隐式泛型调用的类型参数纯粹是编译期装置。
被调方真的在运行期消费 `T`（`typeof(T)` / `new T()` / `default(T)` / `new T[n]`，或转发给
嵌套泛型调用）时，**E0455** 要求显式写出类型实参——把一个静默的错值变成编译错误。
推断失败时静默退化，不产生诊断。

→ [泛型约束](../../reference/src/language/generic-constraints.md)、
[泛型方法](../../reference/src/language/generic-methods.md)；
实现见[泛型的实现](compiler/generics.md)、[泛型类型实参推断](compiler/generic-inference.md)

### `List<T>` / `Dictionary<K,V>` 是真 stdlib 泛型

它们是 `z42.core` 里普通的 `public class List<T>` / `Dictionary<TKey, TValue>`，不是
编译器里硬编码的伪类。编译器里唯一的硬编码是**集合字面量的 desugar 目标名**——只是点名，
不是定义。

→ [集合字面量](../../reference/src/language/collection-literals.md)、
[Std.Collections](../../reference/src/stdlib/collections.md)

## 七、模式匹配

### 结构化模式长在 `switch` 上，没有第二个关键字

`switch`（语句 + 表达式）与 `is` 共用一套 Rust 风格结构化模式：通配、常量、类型、
record 位置解构、属性模式、嵌套、裸绑定、`if` 守卫，外加 or-模式 `|`、`@` 绑定、
闭区间 `..=`、关系模式 `> 0`。

**为什么不引入 `match`**：`switch` 已经在那儿，模式文法可以直接长上去。多一个关键字
= 多一份文法、多一份教学负担，换不来任何表达力。record 的位置解构也不需要
`Deconstruct` 方法或 `out` 参数——主构造器的声明序就是解构序。

穷尽性目前是**警告 W0700**，覆盖 `bool` / `enum` / 封闭的非公开类层次；`sealed` 不在范围内。
强制穷尽要等原生 ADT。

→ [模式匹配](../../reference/src/language/pattern-matching.md)

## 八、不可变性

### 不可变性做在字段上，不做在局部变量上

`readonly` 字段：只能在声明类的实例构造器里（经 `this.<field>`）或字段初始化器赋值，
其它位置赋值报 **E0415**。`const` 是编译期常量。局部变量一律可变。

**为什么先做 `readonly` 而不是 `let`**：`readonly` 是对优化器 ROI 最高的不可变性原语——
它把 `FieldGet` 从 `IsPure` 排除项里放出来，循环里读自己的 readonly 字段可以提升出去。
局部变量的 `let` / `mut` 是纯人体工学收益，且要与将来的 Trait / ADT 设计一起定型。

它是**字段槽位**不可变，不深冻被引用的对象——与 C# `readonly` 同义。

**已知边界**：跨 zpkg 导入的 readonly（要 zbc / zpkg 格式 bump）、非 `this` 接收者的
循环不变外提（要非空分析）、`readonly struct` 都还没有。

→ [readonly 字段](../../reference/src/language/readonly-fields.md)；
优化侧见[优化管线](runtime/optimization-pipeline.md)

## 九、并发

### 真 OS 线程 + 库级原语，没有语言级关键字

`Thread` / `Channel<T>` / `Mutex<T>` / `RwLock<T>` / `Timer`，全在 `z42.threading` 包里。
没有 `lock` 关键字，没有 `async` / `await`，没有 `Task`，没有线程池。

**为什么临界区不做成关键字**：`Mutex<T>.With(body)` 用闭包表达同一件事，零新语法。
`lock` 关键字还会诱导「锁住任意对象」这种 C# 式反模式——`Mutex<T>` 把锁和它保护的数据绑在一起。

**为什么 async 还没有**：染色（async/await 显式）+ 全 async-only stdlib + 结构化并发强制 +
`Send`/`Sync` 类型层安全，是一个整体设计，半套不如没有。

⚠️ 词法器认得 `async` / `await` 两个 token，但语义层没有任何消费者。`async void Foo() { }`
**能编过**——`async` 被当成一个无操作的修饰词吃掉（实测）。这是**静默接受，不是支持**，
别被关键字表误导。

⚠️ 线程间**共享 GC 堆与静态字段**，**数据竞争由程序员负责**。「类型系统防竞争」不是当前状态。

→ [z42.threading](../../reference/src/stdlib/threading.md)；
长期设计见[并发与 async](runtime/concurrency.md)

## 十、模块与产物

### 文件级 `namespace` + 文件级 `using`，另配两个逃生舱

每个文件声明一个 namespace（C# 10+ 文件作用域写法）。`using` 严格**文件级**生效。
两个逃生舱：`global using`（包级 prelude）、`using Id = T;`（文件级类型别名，不导出）。

**为什么是文件级而不是包级**：包级可见意味着「这个名字从哪来」不可局部判定——
删掉兄弟文件的一行 `using` 会让不相关的文件神秘编译失败。文件级与
C# / Rust / Python / Go / TS 一致；真正需要团队 prelude 的场景由 `global using` 显式承担，
而不是靠副作用。

命名空间跨包**不允许循环依赖**。

→ [命名空间与导入](../../reference/src/language/namespaces.md)

### 两种产物，两个职责

| 产物 | 职责 |
|---|---|
| `.zbc` | 编译单元。自带命名空间头与元数据，可独立加载执行；也是增量构建的内容寻址缓存 |
| `.zpkg` | 分发单元。indexed（开发期，索引 `dist/` 下散放的 `.zbc`）与 packed（发布期，内联模块字节码）两形态 |

**确定性 zbc**：`.zbc` 是「源内容 + 编译器版本」的纯函数。源哈希不变则重编产出逐字节相同——
内容寻址增量构建、跨机缓存、自举的 byte-identical 对账全都建立在这条上。

**为什么 packed 要做跨模块去重**：一个包里几十个模块共享大量字符串与签名。packed 形态把
全包字符串并成一个 zpkg 级池、把签名表上提，各模块条目只存池下标。类型表**没有**上提——
按模块内联，因为跨模块的类型描述重合度远不如字符串。

→ 格式规格见 [zbc 字节码格式](formats/zbc.md)、[zpkg 包格式](formats/zpkg.md)；
清单字段见[工程清单 z42.toml](../../reference/src/toolchain/z42-toml.md)

## 十一、执行与部署

### 执行模式是产物属性，不是源码 attribute

三档 interp / JIT / AOT 见[设计哲学](philosophy.md)。落到实现上，执行模式是
**zbc 的每函数元数据字节**加 CLI 旗标，**不是**源码里的 attribute——编译器的内建 attribute 集
（`Suppress` / `Native` / `Deprecated` / `Record` + 测试族）里没有 `ExecMode`，IrGen 目前对所有
函数一律发 `Interp`。命名空间级模式声明是设计方向，尚无语法。

JIT 只在 cargo feature `jit` 打开时可用；AOT 后端是自述的 stub，规划的后端是 **cranelift-object**
（与 JIT 共享翻译层），不是 LLVM。

→ [执行模型](runtime/execution-model.md)、[JIT 后端](runtime/jit-design.md)、[AOT 后端](runtime/aot.md)

### 单一命令入口

`z42` 是 SDK 的唯一用户命令入口，12 个子命令。没有面向用户的「直接跑一个 `.zbc`」命令面——
`z42c` / `z42b` / `z42vm` 是 `z42` 背后的三个进程，各自的命令面归工具链。

**为什么收成一个入口**：SDK 是单版本的，launcher 不管理多个运行时版本；
安装与更新由安装脚本负责，不占命令面。

→ [`z42` 命令面](../../reference/src/toolchain/cli-z42.md)、
[z42c 与 z42b](../../reference/src/toolchain/cli-z42c-z42b.md)

### 运行时行为由登记表驱动的旋钮控制

每个 `Z42_*` 旋钮在旋钮登记表里登记**一次**，CLI / 环境变量 / 两个配置文件层 / 查询命令 /
z42 脚本只读面全部由该表派生。设了一个本 build 不支持的旋钮会得到明确告知，而不是静默无效。

**为什么是登记表而不是各处分别解析**：旋钮有五个来源层，分散解析必然漂移——
某个层少支持一个旋钮、或者两个层语义不一致，都不会有任何东西报错。

→ 旋钮清单与取值语义见[运行时设置](../../reference/src/toolchain/runtime-settings.md)；
五层如何归并见[运行时设置的实现](runtime/runtime-settings.md)

## 十二、标准库

### `Std.*` 是用户面，`Z42.*` 是工具链自用

两类库同住 `src/libraries/`，**命名空间是唯一区分**。包名一律 `z42.*`，用户面命名空间一律 `Std.*`
——两者不是一一对应（`z42.core` 一个包就导出 `Std` / `Std.IO` / `Std.Collections` /
`Std.Threading` / `Std.Time` / `Std.Reflection` / `Std.Runtime` / `Std.Net.Sockets` 八个）。

**为什么不把工具链库藏到别的目录**：它们跟 stdlib 走完全一样的编译、打包、测试路径，
分目录只会分叉构建逻辑。用命名空间区分，一眼可判，零机制成本。

### `z42.core` 是隐式 prelude，但「免 `using`」只有两个命名空间

编译器与 VM 无条件注入 `z42.core`——找不到它什么都跑不起来。但**免 `using` 的只有 `Std`
与 `Std.Runtime`**；同在这个包里的 `Std.IO` / `Std.Collections` 等仍要显式 `using`。

**为什么不把整个 core 都放进免 `using` 集**：prelude 的价值是「基础类型与协议随处可见」，
不是「所有名字随处可见」。`Console` 这种有命名空间归属的类型进了 prelude，
就等于宣布 `Std.IO` 这层划分没有意义。

### 借三家的长处

| 来源 | 借什么 |
|---|---|
| **C#** | 命名约定（`PascalCase`、`Console.WriteLine`）、BCL 结构、`IEquatable` / `IComparable` 协议、`StringBuilder` |
| **Rust** | 抽象边界的纪律、`impl` 块、迭代器链（方向）、零成本抽象作为设计目标 |
| **Python** | batteries included——常见任务不该需要第三方包；模块名可读且扁平；`assert` 是一等开发工具 |

### native 表面受预算约束

**runtime 提供 primitive，feature 一律脚本实现。** primitive = JIT 消不掉的硬能力
（syscall / libm / GC barrier / 类型元数据 / UTF-8 codepoint 访问 / 数值字面量 parse）；
feature（集合 / 算法 / 格式化 / Assert / Path 字符串操作 / 算术辅助）一律脚本。
「这样写更快」与「Rust 内部实现更好」都不是下沉的理由。

新增一个 `extern` 前必须回答「BCL / Rust 把它当 primitive 吗」——回答不出就拒绝。
起手先看 `src/libraries/README.md` 的「Extern 现状审计表」，第一选择是**消减**一个 extern。

→ [实现分层与 native 预算](stdlib/architecture.md)、[包划分与依赖层级](stdlib/organization.md)、
[API 设计准则](stdlib/api-guidelines.md)

## 十三、几个 z42 独有的构造

这些在 C# / Rust 里都没有直接对应物，提案时容易被当成「多余的糖」——它们各自解决一个
z42 结构性问题。

### `available!()`：版本 skew 下的唯一显式豁免

**决策**：缺符号在用到那一刻抛可 catch 的 `Std.MissingSymbolException`；`available!()`
是这条规则的唯一豁免——被它保护的分支整块剪掉，其中的符号永不参与解析。

**为什么需要**：包 A 编译时依赖 B v2，部署时 `libs/` 里却是 B v1。没有豁免通道的话，
「先探测再降级」本身就会因为探测代码引用了不存在的符号而炸掉。

### `methodof`：与 `typeof` 对称

**决策**：`methodof` 求值为 `Std.Reflection.MethodInfo`，与 `typeof(T)` 指代类型对称。

**为什么需要**：没有它，拿一个 `MethodInfo` 只能 `typeof(X).GetMethods()` 再按字符串名筛——
重载筛不开，方法改名也不报错。`methodof` 把方法引用变成编译期检查的。

### 集合字面量：花括号归 List / Dictionary，方括号一律数组

**决策**：`[1, 2, 3]` / `[0; n]` / `[..a]` 一律是数组 `T[]`；`{}` 在表达式位置是
List / Dictionary 字面量。

**为什么两套括号**：数组是语言内建的连续存储，List / Dictionary 是 stdlib 类型——
让括号形状直接编码这个归属，读代码时不必回头看声明类型。

→ [`available!()`](../../reference/src/language/available-macro.md)、
[`methodof`](../../reference/src/language/methodof.md)、
[集合字面量](../../reference/src/language/collection-literals.md)、
[数组](../../reference/src/language/arrays.md)

## 十四、可裁剪性

三条正交的裁剪轴——VM 组件化、stdlib 树摇、语言特性开关——见[设计哲学 · 语言级可裁剪](philosophy.md)。
三条都尚未实施，各有独立设计页。

需要知道的现状：`LanguageFeatures` 类存在且有 21 个开关位，但**零调用方**，
工程清单也不解析任何 `[language]` / `[syntax]` 节。它是设计留痕，不是可用功能。

→ [组件化运行时](runtime/componentized-runtime.md)、[语法定制：三层配置机制](compiler/syntax-customization.md)

---

## 还没有的东西

按「为什么还没做」分组。排期在 `docs/roadmap.md`，不在这里。

**等一整套设计齐了再一次性做**（半套不如没有）：

| 特性 | 依赖谁 |
|---|---|
| `Result<T, E>` + `?` 传播 | ADT + 穷尽 match |
| 原生 ADT / 穷尽 `match` | 类型系统的 sum type 支持 |
| `Option<T>` | 同上（与 `Result` 同一批） |
| `async` / `await` + `Task` | 染色 + async-only stdlib + 结构化并发 + `Send`/`Sync` |
| `Send` / `Sync` 类型层并发安全 | 同上 |
| 空安全流分析 | `Option<T>` 或等价的可空性类型 |

**纯人体工学，ROI 还不够**：

- 局部变量不可变性（`let` / `mut`）
- 迭代器链 / LINQ 风格组合子
- 泛型变型（协变 / 逆变）推理

**方向已定、尚未实施**：

- 命名空间级执行模式声明（`[ExecMode]` 之类的语法）
- hot reload 的 VM 实现（设计页在，`src/` 里零命中）
- AOT 后端（cranelift-object）
- VM 组件化 / stdlib 树摇 / 语言特性开关
- 跨 zpkg 导入的 `readonly`（要格式 bump）、`readonly struct`
- 解构声明 `Point(x, y) = p`、`with` 非破坏性更新、init-only 属性
