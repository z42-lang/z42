# 诊断码全表

> **对齐**：2026-09-22 ｜ **状态**：L1–L2 🚧
>
> z42 编译器可能报出的**全部**诊断码：错误（`E`）、警告（`W`）、信息（`I`），
> 外加保留但当前未接线的工作区清单码（`WS`）。

---

## 这张表怎么来的

| 步骤 | 做法 |
|---|---|
| **码的来源** | [`src/libraries/z42c.core/src/DiagnosticCodes.z42`](../../../../src/libraries/z42c.core/src/DiagnosticCodes.z42) 里的码常量 —— **这是唯一 SoT**。每一个发得出去的码都必须在那里登记，由 `xtask test diagcodes` 强制（见下） |
| **含义** | 取**发射点的诊断消息文本**，而不是常量名。常量名有过一码两义、也有过名实不符（见 `[Forward]` 一节） |
| **状态** | 对每个码做 `grep -rn 'DiagnosticCodes.<常量名>' src/` + `grep -rn '"<码号>"' src/`，排除 `DiagnosticCodes.z42` 自身与 `tests/` 目录 |
| **唯一性** | `xtask test diagcodes`（GREEN gate stage）**活体对账**：① 登记表内无重复码值；② 发射出去的每个码都必须在登记表里登记；③ `DiagnosticCodes.<Name>` 引用的常量必须存在；④ 字面量发码站点清单 `scripts/test/diag-literal-emitters.txt` 双向棘轮；⑤ **本页的码表与登记表双向相等**（本页多一个码 = 有号被占在文档里而登记表看不见；登记表多一个码 = 新码没进本页） |

### 状态列的三个值

| 记号 | 含义 |
|---|---|
| ✅ | **生效** —— 仓库里有实际发射点（给出 `file:line`），写出来的规则会真被强制 |
| ⚠️ | **已定义未接线** —— 码常量存在，但**全仓零发射点**。⚠️ **这些码当前不会被报出**：对应的违规写法编译器**不会拦**，别把它们当成生效的保护 |
| ❌ | **已退役** —— 编号曾存在，现已从源码中整体删除，仅登记以防编号被复用 |

> 🔴 **为什么必须区分**：「⚠️ 已定义未接线」的码占全表约三成。把它们写成生效规则，会让人以为
> `(int)true`、`catch (NotAnException e)` 这类写法有编译期保护——实际上编译器一声不吭地放行。

### 🔴 一码两义曾经发生过两次

**诊断码是用户可见契约**：拿到 `E0477` 就会来这张表查它是什么意思。一码两义 ⇒ 查到的是
**另一个诊断的解释**——比「查不到」更坏，因为它看起来是个答案。

2026-09-22 实测，main 上同时躺着两处（均已由 `enforce-diagnostic-code-uniqueness` 按
**先来后到**归位，后到者改号）。**第三处是这道门上线当天自己抓到的**——#747 与 #759 两个并行
PR 前后脚合入、各拿了一个 E0481，git 毫无反应，门在 main 上第一次跑就红了（这正是它存在的理由）：

| 码 | 先来（保号） | 后到（改号到） |
|---|---|---|
| E0474 | 属性混合 auto 与带体访问器（#737） | 值类型与 `null` 比较 → **E0481**（#741） |
| E0477 | 取重载自由函数引用无匹配（#745） | 赋值目标不是左值 → **E0482**（#749） |
| E0481 | 接口声明了非法成员（#747，早两个 commit 合入） | 值类型与 `null` 比较 → **E0483**（#759） |

成因是机制而非粗心：发码点可以绕开登记表（用字面量），于是一个码能「被发射出去」却
**从不进登记表**；后来者扫登记表找空位，看不见那些字面量码，就挑中一个已被占用的号。
两个并行 PR 各自在自己的文件里写下同一个号时，git 眼里是两处互不相干的新增 ⇒ **欢快合并**。
`xtask test diagcodes` 就是补上这个缺席的信号。

### 当前没有 `explain` 命令

`z42c` **没有** `explain` / `errors` 子命令（本页历史版本声称有，实为未实施）。查码请直接用本页。

---

## E01xx — 词法（Lexer）

发射点全在 [`src/libraries/z42c.syntax/src/Lexer.z42`](../../../../src/libraries/z42c.syntax/src/Lexer.z42)。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0101 | 字符串字面量未闭合（普通 / raw / 插值三种都走这个码） | ✅ `Lexer.z42:254,286,320` | `string s = "abc` |
| E0102 | 无法识别的转义序列 | ✅ `Lexer.z42:344` | `"a\q"` |
| E0103 | 非法数字字面量 / 意外字符 | ✅ `Lexer.z42:416` | `0x`（无数字） |

---

## E02xx — 语法（Parser）

发射点在 `src/libraries/z42c.syntax/src/` 下的 `Parser.z42` / `ExprParser.z42` / `DeclParser.z42` /
`TypeParser.z42` / `MemberParser.z42` / `MethodOfParser.z42`。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0201 | 该位置不接受这个 token（顶层非声明、表达式里出现无法使用的 token） | ✅ `Parser.z42:416`、`ExprParser.z42:503,519,560` | 顶层写 `foo;` |
| E0202 | 缺少必需的 token（类型名 / `>` / 枚举成员名 / `->` …） | ✅ `TypeParser.z42:30,144,192`、`DeclParser.z42:129,260,273` | `class { }` |
| E0203 | 输入在构造完成前结束 | ✅ `Parser.z42:184,196,208` | 文件以 `class C {` 结尾 |
| E0204 | 函数声明缺返回类型 | ⚠️ 零发射点 | — |
| E0205 | 表达式无法无歧义解析 | ⚠️ 零发射点 | — |
| E0206 | `params` 不是最后一个形参 | ✅ `MemberParser.z42:380` | `void f(params int[] a, int b)` |
| E0207 | `params` 形参类型不是数组 `T[]` | ✅ `MemberParser.z42:369` | `void f(params int a)` |
| E0208 | `params` 与 `ref`/`out`/默认值同时出现 | ✅ `MemberParser.z42:371` | `void f(params ref int[] a)` |
| E0209 | 只能出现在文件顶部的声明（`using` / `namespace`）出现在语句位置。此前会级联出 4 条无关错误，没有一条说得出真正原因 | ✅ `Parser.z42:246` | 在方法体里写 `using Std.Text;` |

下面三个码编号落在 E04xx，但实际由**语法层**报出，在此一并登记（E04xx 表不再重复）：

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0442 | 顶层声明标了 `private` / `protected`（模块作用域下无意义，用 `internal` 或 `public`） | ✅ `Parser.z42:395` | 顶层 `private class C { }` |
| E0457 | 一个文件里写了**多个**文件级 `namespace X;`，或把 `namespace` 写在类型/函数声明**之后**。此前是**静默 last-wins**，会导致限定名解析到错误的类型。恢复策略：保留**第一个** ns | ✅ `Parser.z42:345,349` | `namespace A; class C { } namespace B;` |
| E0462 | `methodof(...)` 括号内**签名语法**形态错：缺 `Type.Member`、owner 带类型实参、方法带类型实参 | ✅ `MethodOfParser.z42:32,54,60,66,71` | `methodof(Logger)` |

---

## E03xx — 特性开关

| 码 | 含义 | 状态 |
|---|---|---|
| E0301 | 使用了未启用的语言特性 | ⚠️ 零发射点 |

---

## E04xx — 语义 / 类型检查

发射点全在 `src/compiler/z42c.semantics/src/`（本节表中的路径均相对该目录），
E0442 / E0457 / E0462 除外（见上一节）。

### 基础诊断

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0401 | 未定义符号：变量 / 函数 / 字段 / 方法找不到 | ✅ `MemberResolver.z42:106,138,377,707`、`PatternBinder.z42:324` | 调用未声明的 `foo()` |
| E0402 | 类型不匹配（含不支持的语句 / 模式、空集合字面量缺目标类型等兜底场景） | ✅ `StmtBinder.z42:368,391`、`CollectionTyper.z42:35,51,132,141`、`PatternBinder.z42:32` | `var a = [];` |
| E0403 | 非 void 函数存在无 `return` 的路径 | ⚠️ 零发射点 | — |
| E0404 | 访问控制违规：`private`/`protected` 成员跨界访问，或引用 `private`/`protected` 嵌套类型（对标 C# CS0122） | ✅ `AccessChecker.z42:66,192` | 类外读 `private` 字段 |
| E0405 | 非法修饰符组合（如同时写两个访问修饰符） | ✅ `DeclParser.z42:101`（语法层） | `public private int x;` |
| E0406 | 整数字面量超出显式宽度类型的范围 | ⚠️ 零发射点 | — |
| E0407 | **局部变量**未赋值就读。合并规则：`if` 两支都赋才算（一支必定 return/throw 则取另一支）／`while` 体内赋值不算（可能零次执行）／`do-while` 算（必执行一次）／`switch` 取各 case 交集且须有 `default`／`try` 的赋值在 **catch 里不算**（异常可能在赋值前抛出）。字段与静态字段不在管辖（它们零初始化） | ✅ `FlowAnalyzer.z42` | `int x; return x;` |
| E0408 | 重复声明。当前实际发射面是**顶层自由函数重名**（自由函数不支持重载）与成员收集期的重复成员 | ✅ `DeclBinder.z42:49,120`、`MemberCollector.z42:42` | 同文件两个 `int f()` |
| E0409 | 把 `void` 表达式赋给变量 | ⚠️ 零发射点 | — |
| E0410 | `break` 在循环 / `switch` 外，或 `continue` 在循环外 | ✅ `StmtBinder.z42:352,358` | 方法体顶层写 `break;` |
| E0411 | 非法继承 | ⚠️ 零发射点（sealed 相关走 E0427–E0429） | — |
| E0412 | 接口实现不匹配：签名 / `static` 与实例形态不一致 | ✅ `InheritanceResolver.z42:444,457,479,494,499` | 接口声明实例方法，实现方写成 `static` |
| E0413 | 非法实现 | ⚠️ 零发射点 | — |
| E0414 | event 字段的外部访问控制 | ⚠️ 零发射点 | — |
| E0420 | `catch (T e)` 的 `T` 不是 `Exception` 的子类 | ⚠️ 零发射点 —— **catch 类型当前不校验**，`catch (NotAnException e)` 编译器不报错 | — |
| E0421 | 非法的 `default(T)` 目标类型 | ⚠️ 零发射点 | — |
| E0424 | 非法强制转换 | ⚠️ 零发射点。非法 cast 实际走 E0402 / E0439，或运行期 `Std.InvalidCastException` | — |
| E0443 | 类型注解引用了未定义的类型名（对标 C# CS0246） | ✅ `AccessChecker.z42:132`、`ConstraintChecker.z42:212`、`TypeOpTyper.z42:91` | `Nope x = null;` |

### `const` / `readonly` / 属性写入

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0415 | `readonly` 字段在构造函数外（静态字段则在初始化器外）被赋值 | ✅ `AssignTyper.z42:256` | 普通方法里写 `this.RoField = 1;` |
| E0416 | `const` 声明缺初始化器 | ✅ `StmtBinder.z42:269`、`MemberCollector.z42:389` | `const int K;` |
| E0417 | `const` 初始化器不是编译期常量 | ✅ `StmtBinder.z42:278`、`MemberCollector.z42:399` | `const int K = f();` |
| E0418 | 给 `const` 赋值 | ✅ `AssignTyper.z42:178,187` | `K = 2;` |
| E0419 | 常量表达式引用了尚未定义的 `const` | ✅ `StmtBinder.z42:276`、`MemberCollector.z42:396` | `const int A = B; const int B = 1;` |
| E0452 | 给没有 setter 的属性赋值：`X { get; }` 只读自动属性仅可在构造函数内经 `this` 写；`X { get {..} }` 计算属性任何位置都不可赋值（对标 C# CS0200）。静态属性同理，仅本类静态 ctor 内可写 | ✅ `AssignTyper.z42:217,220,268,271`、`DeclBinder.z42:229` | `obj.GetOnly = 1;` |

### 转换与重载

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0425 | 重载歧义：多个候选同等匹配 | ✅ `OverloadBinder.z42:418,667,709,754,784,790`、`ConstructTyper.z42:236` | 两个重载分别取 `int` / `long`，传 `byte` |
| E0426 | `new C(args)` 的实参与本地构造器形参不匹配（`params` / 默认值尾巴均已考虑）—— 防的是静默按位截断 | ✅ `ConstructTyper.z42:165,291` | `new Point(1)`，而 `Point` 只有 `(int,int)` |
| E0437 | target-typed `new()` 的目标类型推断不出来或有歧义 | ✅ `ConstructTyper.z42:26,151`、`OverloadBinder.z42:677,699` | `var x = new();` |
| E0439 | 存在显式转换但用在隐式上下文（窄化 / 有损）—— 必须写 `(T)` cast | ✅ `TypeChecker.z42:368` | `int i = someLong;` |
| E0440 | 转换运算符声明冲突：同一 (源→目标) 重复，或 `implicit` 与 `explicit` 同对 | ✅ `MemberCollector.z42:339,342` | 同类里同时写 `implicit operator int` 与 `explicit operator int` |

### 继承 / `sealed` / `static` 类 / `partial`

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0427 | 继承 `sealed` 类 | ✅ `InheritanceResolver.z42:251` | `class D : SealedBase { }` |
| E0428 | `override` 一个已 `sealed` 的 override 方法 | ✅ `InheritanceResolver.z42:292` | — |
| E0429 | 方法级 `sealed` 没有 override 基类的任何 `virtual` 方法 | ✅ `InheritanceResolver.z42:287` | — |
| E0430 | 同名类型的某个声明没标 `partial`（必须全部标） | ✅ `DeclEnforcer.z42:139` | `partial class C {} class C {}` |
| E0431 | 各碎片的 Kind（class / struct / interface）不一致 | ✅ `DeclEnforcer.z42:143` | `partial class C {} partial struct C {}` |
| E0432 | 基类被多个碎片声明（至多一个碎片可写基类） | ✅ `StubCollector.z42:199` | — |
| E0433 | 碎片之间重复成员（同名字段 / 同签名方法） | ✅ `MemberCollector.z42:176,323` | — |
| E0434 | `partial` 方法：实现找不到匹配的声明，或声明与实现签名不一致 | ✅ `DeclEnforcer.z42:476` | — |
| E0435 | 嵌套类型自身标 `partial`（v1 不支持） | ✅ `DeclEnforcer.z42:160` | — |
| E0451 | `static` 类含实例成员（方法 / 字段 / 属性 / 构造器）、声明了基类、或实现了接口（对标 C# CS0708/0710/0713/0714） | ✅ `InheritanceResolver.z42:262,266,554,557,563,568` | `static class U { public int V; }` |
| E0469 | 实例构造器没写初始化子句（⇒ 隐式 `: base()`），而基类有实例构造器却**无一可零实参调用** ⇒ 须显式 `: base(...)`（对标 C# CS7036）。派生类**没写任何构造器**时不报 | ✅ `DeclBinder.z42:516` | `class B { public B(int x){} } class D : B { public D(){} }` |
| E0470 | `ref` 实参不是可取址的左值（属性 / 索引器 / 静态字段 / 值 struct 的字段 / 字面量 / 调用结果 / 数组虚成员）。这些形态**此前编译通过但写回静默丢失**——取的是承载读出值的临时寄存器的地址 | ✅ `ExprTyper.z42`（`_chkRefArgLvalue`）| `void Inc(ref int x){} ... Inc(ref h.P)` |
| E0471 | 使用了 `out` / `in` 作参数修饰符（形参位或调用点）。三态已收敛为单一 `ref`：`out` 的四条规则全为处理「未初始化内存」这一个例外，而槽位自动取零值消灭了该例外；`in` 的只读保证从设计时起就不完整（只约束 slot 不可重赋，不约束指向对象的内部状态）。诊断附迁移写法 | ✅ `MemberParser.z42`（形参侧）/ `ExprParser.z42`（调用点） | `void F(out int v){}` → `void F(ref int v){}` |
| E0472 | 形参是 `ref`，**实参漏写** `ref`。此前编译通过且方法里的写入**静默丢失**——被调方改的是自己的形参寄存器，出口 copy-out 没有调用方的 lvalue 可写回 | ✅ `OverloadBinder.z42`（`_checkRefSymmetry`）| `void Inc(ref int x){} ... Inc(v)` |
| E0473 | 形参**不是** `ref`，实参却多写了 `ref`。与 E0472 方向相反但同样有害：此前编译通过且写入**传回了调用方**，即「按引用与否由调用点决定」，光看函数声明判断不出参数会不会被改 | ✅ `OverloadBinder.z42`（`_checkRefSymmetry`）| `void ByValue(int x){} ... ByValue(ref v)` |
| E0474 | 属性的两个访问器**混合** auto 与带体（一半 `get;`/`set;`、另一半 `get { }`/`set { }`）。z42 无 C# 的 `field` 关键字，auto 半边读/写合成后备 `__prop_X`、带体半边写自备字段 → 读写错位。须**要么都 auto、要么都带体** | ✅ `MemberParser.z42`（`_parseProperty`，字面量发码；常量 `MixedPropertyAccessors`） | `int P { get; set { _x = value; } }` |
| E0475 | 把可空表达式隐式转给不可空的值类型目标。`?` 擦除后此前一路放行，运行期才以 `type mismatch in arithmetic: Null vs I64` 之类的**内部错误**炸出来 | ✅ `TypeChecker.z42`（`CheckImplicitConvert`，字面量发码） | `int? m = null; int y = m;` |
| E0476 | 对**值类型**写 `?`（`int?` / `Guid?` / 值 struct）。可空只适用引用类型——`?` 对值类型此前是个「看起来存在、实际为零」的标注。⚠️ `byte[]?` 这类**数组**不受限（数组是引用类型） | ✅ `TypeParser.z42`（字面量发码） | `int? m = null;` |
| E0477 | 取一个**重载自由函数**的引用时，目标委托类型在场，但**没有任何重载**的签名（形参逐位 + 返回）与该委托**精确相等**。诊断列出该名字下全部候选签名。见 [delegates §2.5](../language/delegates-events.md#25-重载自由函数取引用按目标委托消解) | ✅ `ExprTyper.Funcref.z42`（`_bindFuncRefTargeted`，字面量发码；常量 `FuncRefNoMatchingOverload`） | `Func<string,bool> b = Parse;`，`Parse` 无 `(string)->bool` 重载 |
| E0478 | 解引用一个标了 `?` 的形参，而此前没有检查过空值。`?` 的语义是「**请编译器在这里强制检查**」——**不标就不强制**，所以存量代码一行不用改。逃生口是窄化：`if (s != null) { … }` / 早返回守卫 `if (s == null) { return; }` / `s != null && s.X` / 三元。**没有 `!` 那样的「我保证」后缀**（那正是要避开的逃逸口）。覆盖**裸名**解引用与**调用结果**解引用；事实来源是标 `?` 的**形参**与标 `?` 的**返回值**。字段（需先定快照规则）见 `define-null-check-marks` 的后续 PR | ✅ `FlowAnalyzer.z42`（字面量发码） | `int M(string? s) { return s.Length; }` |
| E0479 | 把「可能为 null」的值 `return` 给**未标 `?`** 的返回类型。未标的返回类型意味着「调用方不必检查」，放行就等于凭空造一个洞。两条修法诊断里都给：给返回类型加 `?`（把义务传给调用方），或在这里先检查。⚠️ 只认**确定性**来源（标 `?` 的名字 / 标 `?` 的调用结果）；裸 `return null;` **不报**——那是「建议加 `?`」的反向推导，另有其码 | ✅ `FlowAnalyzer.z42`（字面量发码） | `string M(string? s) { return s; }` |
| E0482 | 赋值目标不是左值（没有可写的存储）：`42 = a` / `f() = x` / `(A, B) = (a, b)` 在**表达式位置**。此前这道检查根本不存在——三种全都编得过、跑得过、什么也不发生、零诊断。最伤人的是表达式体成员 `Pair(int a, int b) => (A, B) = (a, b);`：读起来完全像给两个字段赋值，实际字段全 0。⚠️ 与 **E0470**（`ref` 实参左值）不是一回事：那条要「可取址」，严得多；赋值只要「有存储」。⚠️ 本码**原为 E0477**，与「取重载自由函数引用无匹配」（先占号）撞码，2026-09-22 按先来后到改号 | ✅ `AssignTyper.z42`（`_checkAssignable`，字面量发码；常量 `AssignTargetNotLvalue`） | `42 = a;` |
| E0483 | 值类型表达式与 `null` 比较（`==` / `!=`）。值类型永不含 null ⇒ 该比较是**静默恒假/恒真**，此前零诊断。⚠️ 本码**原为 E0474**（与「属性访问器混合 auto 与带体」撞）→ 改号 E0481 → 又撞 #747「接口非法成员」（那边早两个 commit 合入、保号）→ 再改号到此。两次都是因为它的发射点用字面量、号在登记表里没有主，抢号的人看不见它 | ✅ `TypeChecker.z42`（`_checkValueTypeNullCompare`，字面量发码；常量 `ValueTypeNullComparison`） | `int x = 1; if (x == null) { }` |
| E0480 | 使用了已移除的空值运算符 —— `?.`（空条件成员访问）或 `??`（空合并）。**两者同码**：它们是同一个口子的两种写法，都把「可能为 null」静默收尾掉。诊断给迁移写法，并把表达式按等价合法形态解析完（`?.` 按 `.`、`??` 只取左侧）以免级联错（同 E0471 对 `out`/`in` 的手法）。「读设置取默认值」优先换成接受默认值的 API（全仓 70 处 `GetEnvironmentVariable("X") ?? ""` 即如此迁移）。⚠️ 顺带修掉一个真 bug：`?.` 旧脱糖把接收者**绑定两次** ⇒ `F()?.X` **调用 `F` 两次** | ✅ `ExprParser.z42`（字面量发码） | `var v = n?.value;` / `string s = a ?? b;` |
| E0481 | 接口声明了**非法成员**——字段（静态/实例）或嵌套类型。接口只能声明方法、属性、索引器、事件、关联类型。此前 `_fillInterface` 静默跳过这些（`interface I { static int X; }` 编译通过但 X 无处可用） | ✅ `MemberCollector.z42`（`_fillInterface`，字面量发码） | `interface I { static int X; }` |
| E0484 | 直接解引用一个标了 `?` 的**字段 / 属性**，而没有先快照到局部。与 E0478（形参）分成两码，因为**修法不同**：形参就地 `if (s != null) { … }` 就够，字段不行 —— 字段不是一个「值」，是一个**每次读都重新求值的位置**：`if (this.F != null) { this.F.M(); }` 里的两个 `this.F` 是两次独立的读，别的线程能在中间写；`F` 若是属性还是两次真调用，返回值可以不同。所以字段**永不就地窄化**，唯一修法是 `var v = this.F; if (v != null) { … }`（局部是个值，检查一次就永远成立）。⚠️ 跨包暂不携带标记（字段的 TSIG 拼写已擦除 `?`）⇒ 导入字段视为未标，是**漏报**方向 | ✅ `FlowAnalyzer.z42`（字面量发码） | `class C { string? F; int M() { return this.F.Length; } }` |
| E0485 | 一个包里出现了**第二个** `[ModuleInit]`。包级初始化器至多一个 —— 多处装配写在同一个方法里，顺序才是显式的。诊断报在后出现的那处，并指出第一处的 `file:line`。**包级判定**：跨 CU，per-file 阶段看不见 | ✅ `ModuleInitScan.z42`（`CheckPackage`，字面量发码；常量 `ModuleInitDuplicate`） | 同一包两个文件各写一个 `[ModuleInit]` |
| E0486 | `[ModuleInit]` 标注目标非法：包初始化器必须是**有体、无参、返回 `void`、非泛型**的方法；类内成员还必须 `static`（顶层自由函数豁免 —— 它本就无 this、恒 `IsStatic=false`）。诊断带上「哪里不对」那一条 | ✅ `ModuleInitScan.z42`（`CheckPackage`，字面量发码；常量 `ModuleInitBadTarget`。⚠️ 本码原取 E0484，与「解引用标了 `?` 的字段/属性」撞 —— 那边早合入 main、保号，本码按先来后到让到 E0486） | `public class C { [ModuleInit] void Init() { } }` |

### 泛型 / 约束 / 关联类型

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0422 | 类型实参不满足函数类型约束 | ✅ `ConstraintChecker.z42:474` | — |
| E0423 | 型参把函数类型约束与其他约束混写，或同时要求 `class` + `struct` | ✅ `ConstraintChecker.z42:231` | `where T : (int)->int, IFoo` |
| E0446 | 泛型方法调用 `Foo<...>()` 的类型实参数与声明的类型形参数不符 | ✅ `MemberResolver.z42:613` | `Swap<int, string>(a, b)`，而 `Swap<T>` 只有一个型参 |
| E0453 | 关联类型：接口里 `type Item;` 未被实现方绑定；实现方写了不带绑定的 `type Item;`；接口自己写了绑定；`where T : IFoo<Zzz = int>` 里 `Zzz` 不是该接口的关联类型；实参绑定与约束要求不符 | ✅ `ConstraintChecker.z42:168,500,506`、`InheritanceResolver.z42:363`、`MemberCollector.z42:70,160` | — |
| E0454 | 经**接口静态类型**调用形参位含 `Self` 的方法。`Self` 在形参位是逆变方向、没有唯一安全上界，放行即运行期类型混淆。替代写法是型参：`bool f<T>(T a, T b) where T : IEq` | ✅ `MemberResolver.z42:127` | `IEq a, b; a.Same(b);` |
| E0455 | 省略尖括号调用一个**方法体真的消费型参**的泛型方法（体内有 `typeof(T)` / `new T()` / `default(T)` / `new T[n]`，或把 `T` 转发给嵌套泛型调用）。这类 callee 在运行期会读到**空**的方法类型实参；本码把静默错值换成编译错误，修法是显式写出 `<T>` | ✅ `MemberResolver.z42:583` | `Make()` → 应写 `Make<int>()` |
| E0463 | 在**型参收者**（`where T : IEq` 的 `T`）上调用约束接口方法时，给形参位是 `Self` 的形参传了**具体类型**实参。与 E0454 的分工：E0454 管接口静态类型收者，本码管型参收者 | ✅ `OverloadBinder.z42:180` | `a.Same("nope")`，`a` 的类型是 `T` |

### 名字解析

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0436 | 用了某依赖命名空间却没在本文件 `using`（file-scoped usings） | ✅ `CuPreprocess.z42:186` | — |
| E0441 | 不一致可访问性：高可见性的成员 / 类型签名暴露了更低可见性的类型（对标 C# CS0050 族） | ✅ `AccessChecker.z42:227` | `public void f(InternalOnly x)` |
| E0456 | 非限定短名同时匹配**多个可见命名空间**里的类型（对标 C# CS0104）。当前命名空间里的那一份优先，限定写法永不歧义。此前是**静默择一**，选中哪份取决于加载顺序 | ✅ `SymbolCollector.z42:449`、`TypeChecker.z42:158,163` | `using A; using B;` 后裸写 `Box b = null;`，A/B 各有一个 `Box` |
| E0458 | 同一命名空间里重复声明同一个类型（同 ns、同名、同 arity，且并非全部 `partial`；对标 C# CS0101）。判据是 **(ns, 名字, arity)** 三者都相同。此前是**静默 last-wins**，前一个连同成员一起消失 | ✅ `StubCollector.z42:231` | 两个文件各写一个 `namespace X; class Config` |

### 编译期宏 / `methodof` / 属性与生成器

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0444 | `: Attribute` 的类名必须以 `Attribute` 结尾 | ✅ `DeclEnforcer.z42:26` | `class My : Attribute { }` |
| E0445 | `: Analyzer` 的类名必须以 `Analyzer` 结尾 | ✅ `DeclEnforcer.z42:52` | — |
| E0447 | `: Generator` 的类名必须以 `Generator` 结尾 | ✅ `DeclEnforcer.z42:73` | — |
| E0448 | generator 只能改它 trigger 命中的声明（Augment / Replace 越界） | ✅ `GeneratorDriver.z42:273,281,319,327,496` | — |
| E0449 | generator 的 consumes / produces 依赖成环，拓扑分层无法定序 | ✅ `GeneratorDriver.z42:531` | — |
| E0450 | 编译期宏 `name!()` 误用：未知宏名 / 参数类型不符 / 出现在非参数默认值位 | ✅ `DeclBinder.z42:566,569,577,579,586`、`ExprTyper.z42:88,417` | 形参默认值写 `nope!()` |
| E0459 | `methodof(T.M)` 的目标方法不存在，或没有重载匹配给出的参数类型列表。诊断会列出该名字下全部可用重载 | ✅ `TypeOpTyper.z42:149,173` | `methodof(Logger.Nope)` |
| E0460 | `methodof` 无法唯一确定目标：给了参数类型列表却匹配到多个，或省略了参数列表而候选 ≥ 2。**绝不静默择一** | ✅ `TypeOpTyper.z42:181,191` | `methodof(Logger.Log)`，`Log` 有两个重载 |
| E0461 | `methodof` 的目标是**指不了**的方法：用户定义的运算符与转换在源码里没有名字（`op_*` 是编译器内部拼写） | ✅ `TypeOpTyper.z42:104` | `methodof(Vec.op_Addition)` |

### `[Forward]` 转发生成（⚠️ 常量名与实际发射不符，以本表为准）

`DiagnosticCodes.z42` 里 `ForwardTargetNotFound = E0464` / `ForwardNotRenderable = E0465` /
`ForwardAmbiguous = E0466` 这组**常量名与实际发射不一致**（`ForwardSkipped` 已于 2026-09-22 改值归位到 I0466）。
实际发出来的是下面四个，含义取自
[`ForwardGenerator.z42`](../../../../src/compiler/z42c.semantics/src/ForwardGenerator.z42)
的诊断文本：

| 码 | 实际含义（按发射点消息） | 状态 |
|---|---|---|
| E0464 | 转发面**渲染不出等价签名**：目标成员是 `private`（不该把别人的私有实现抬到自己的公开面上），或它声明在**另一个包**（跨包元数据既不带参数名也不带 `ref`/`out`，丢掉 `ref` 会**静默**产生错误语义）。修法：改用 `partial` 声明档 | ✅ `ForwardGenerator.z42:232,237` |
| E0465 | 转发目标**不可转发或不唯一**：点名了 Object 协议方法（`ToString` / `Equals` / `GetHashCode` / `GetType`——转发它们会让外层类报告内层的身份），或该名字在目标类型上有**多个重载**（转发要生成一个具名方法，无法代选） | ✅ `ForwardGenerator.z42:194,216` |
| E0468 | `[Forward]` 形态 / 用法错：字段类型不是 class/interface、没有可转发的成员面；`[Forward(...)]` 实参既非 `typeof(接口)` 也非 `methodof(类型.成员)`；`typeof(X)` 里 X 不解析成接口；`methodof(类型.成员)` 点名的成员**不在该字段的类型上** | ✅ `ForwardGenerator.z42:104,126,163,208` |
| **I0466**（Info） | 外层类已自己声明了同名成员 → `[Forward]` **跳过不生成**。这不是错误（用户的实现优先），但必须说出来——否则「贴了 `[Forward]` 却没生效」是一个没有任何解释的缺席 | ✅ `ForwardGenerator.z42:201,279` |
| E0466 | 常量 `ForwardAmbiguous` | ⚠️ 零发射点（重载歧义实际发的是 E0465） |
| I0467 | ❌ **已退役**（2026-09-22）：常量 `ForwardSkipped` 原登记此号而发射点一直发 I0466，改值归位后本号空出，**不复用**（占号常量 `RetiredForwardSkipped`）。⚠️ **前缀不同即不同码**（本行上方 E0466 与 I0466 并存即先例），所以退役的是 `I` 前缀的 0467，**`E` 前缀的 0467 不受牵连、仍是可分配空号** |

### 保留编号

| 码 | 说明 |
|---|---|
| E0438 | 预留给「值 struct 自引用」诊断。常量 `StructSelfReference` **已登记占号、零发射点**；当前由布局计算兜底防崩，自引用 struct 退化为引用语义、不报错 |

> **保留 ≠ 只写在这里**。保留号和退役号一样要在 `DiagnosticCodes.z42` 里登记成常量——占号若只活在本页，`xtask test diagcodes` 就看不见它，下一个扫登记表找空位的人会把它当空号拿走。
> 规则 ⑤ 现在盯着这件事。

---

## E05xx — IR 代码生成

| 码 | 含义 | 状态 |
|---|---|---|
| E0501 | 语法合法但尚未降解到 IR 的构造 | ⚠️ 零发射点 |

---

## E06xx — 包与 import 解析

发射点在 `src/compiler/z42c.semantics/src/`。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0601 | **同一个 FQN 被多个依赖包声明**。与 E0456 的分工：E0456 管「同**短名**跨 ns」（限定写法即可消歧），本码管「**FQN 本身**重复」——写全限定名也分不开，被遮蔽那份在 z42 里没有任何写法能指到。此前是字母序靠前的包赢，输的那份连同全部成员从未存在过，报错还答非所问（「no method X on Widget」） | ✅ `SymbolCollector.z42:461`、`TypeChecker.z42:66,147,154` | 两个依赖包各声明 `Demo.Ns.Widget` |
| E0602 | `using <ns>;` 声明的 namespace 没有任何已加载包提供 | ⚠️ 零发射点 —— **未解析的 `using` 当前不会被报出** | — |
| E0605 | 非 `z42.*` 包在自己源码里声明 `namespace Std.*`（或裸 `Std`） | ⚠️ 零发射点 | — |
| E0606 | **本包声明的类型遮蔽了某个导入包的同 FQN 类型**。失败形态与 E0601 一样（被遮蔽那份根本指不了），但冲突的两份里有一份是本包自己写的、改名随时可以 ⇒ 定为 error 而非 warning | ✅ `SymbolCollector.z42:463`、`TypeChecker.z42:68,149,156` | 本包写了与依赖包逐字相同的 `Demo.Ns.Widget` |
| W0603 | 依赖扫描层：消费一个 NSPC 占用了 `Std.*` 的第三方 zpkg | ⚠️ 零发射点 | — |

---

## E09xx — 编译器内部 / 原生互操作 / 测试框架

### E0900 编译器内部错误

| 码 | 含义 | 状态 |
|---|---|---|
| E0900 | 编译器内部不一致。当前唯一发射点：宏注册表认了某个宏名、但绑定层没有对应分支 | ✅ `ExprTyper.z42:427` |

### E0901–E0916 原生互操作（⚠️ 整组未接线）

> 🔴 这一组码全部源自**已退休的 C# bootstrap 编译器**，自举迁移时**未移植到 z42c**。
> 常量在 `DiagnosticCodes.z42` 里定义齐全，但**全仓零发射点**——
> `extern` 缺 `[Native]`、`[Native]` 形态错、`pinned` 块里写 `return`、`.z42abi` manifest 有问题，
> 这些当前编译器**都不会拦**。

| 码 | 含义 | 状态 |
|---|---|---|
| E0901 / E0902 | **已退役**。原 `UnknownNativeName`（`[Native("__name")]` 不在 VM dispatch_table 内）与 `NativeArityMismatch`（`extern` 形参数与注册项不一致）。C# 编译器删除后这两个编号连常量定义都不存在 —— 现已补上占号常量 `RetiredUnknownNativeName` / `RetiredNativeArityMismatch`（零发射点，仅防复用） | ❌ |
| E0903 | `extern` 方法缺少 `[Native]` 标注 | ⚠️ 零发射点 |
| E0904 | `[Native]` 标注用在非 `extern` 方法上 | ⚠️ 零发射点 |
| E0907 | `[Native(...)]` 形态错（未知键 / 值不是字符串字面量 / 完全无键），或 Tier1 binding 拼接后仍缺 lib / type / entry 任一字段 | ⚠️ 零发射点 |
| E0908a | `pinned p = <expr> { ... }` 中 `<expr>` 类型不是 `string` | ⚠️ 零发射点 |
| E0908b | `pinned` 块体内含 `return` / `break` / `continue` / `throw` | ⚠️ 零发射点 |
| E0909 | `.z42abi` manifest 读取失败：文件不存在 / IO 失败 / JSON 不合法 / `abi_version` 不符 / 缺必需字段 | ⚠️ 零发射点 |
| E0916 | native import 合成失败：`import T from "lib";` 中 T 不在 manifest 的 `types[]`；manifest 的 `params`/`ret` 用了白名单外的类型形态；`*const c_char` 出现在 ret 位；同名 type 被两条 import 声明但 lib 不同；找不到 `<lib>.z42abi` | ⚠️ 零发射点 |

> **运行期** marshal 失败不走错误码：VM 直接抛
> [`Std.InvalidMarshalException`](../../../../src/libraries/z42.core/src/Exceptions/InvalidMarshalException.z42)，
> 脚本侧用 `catch (Std.InvalidMarshalException e) { ... }` 处理，读 `Message` / `StackTrace` 字段。
> 触发场景：字符串含 interior NUL 投到 `*const c_char`；`PinPtr` 源不是 `String` / `Array<u8>`；
> `PinPtr` 数组元素不在 `0..=255`。

### E0911–E0917 测试框架

发射点全在
[`DeclEnforcer.z42`](../../../../src/compiler/z42c.semantics/src/DeclEnforcer.z42)
的 `_passTestAttrEnforce`（纯语法检查，不依赖符号表）与相邻的 `_passTestAttrSemantic`。

强制五条规则：**零接收者**（顶层自由函数或 `static` 方法）、**返回 `void`**、**无参数**、
**非泛型**、**有方法体**。位置违规只报一条即返回；其余四条各报一次。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E0911 | `[Test]` 违反五条规则之一 | ✅ `DeclEnforcer.z42:371` | `[Test] int t() { return 0; }` |
| E0912 | `[Benchmark]` 违反同五条规则（**脱糖后**判定：form-2 `void f(Bencher b)` 合法） | ✅ `DeclEnforcer.z42:369` | — |
| E0913 | `[ShouldThrow]` 缺类型实参，或该类型**可解析但基类链到不了 `Exception`**。⚠️ 类型解析不到时**刻意不报**（跨包 / 符号表不完整会误伤） | ✅ `DeclEnforcer.z42:427,432` | `[ShouldThrow(typeof(int))]` |
| E0914 | `[Skip]` 缺 `reason` 或 reason 为空串；或 `[Skip]`/`[Ignore]` **孤儿使用**（同声明上没有 `[Test]`/`[Benchmark]`） | ✅ `DeclEnforcer.z42:270,303` | `[Skip] void t() {}` |
| E0915 | `[Setup]` / `[Teardown]` 违反同五条规则 | ✅ `DeclEnforcer.z42:370` | — |
| E0917 | `[Timeout]` 缺 `milliseconds`（或该实参非整数字面量），或其值 ≤ 0 | ✅ `DeclEnforcer.z42:278,284` | `[Timeout(0)] [Test] void t() {}` |

> E0913 / E0914 / E0917 修的都是**静默降级**——此前编译器读不到合法实参就取默认值继续走
> （skip 没理由、超时静默失效、抛出类型永不匹配），把问题全推到运行期且症状指不回病灶。

---

## E10xx — 调用实参绑定

发射点在 [`OverloadBinder.z42`](../../../../src/compiler/z42c.semantics/src/OverloadBinder.z42)。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E1001 | 位置实参出现在具名实参之后 | ⚠️ 零发射点 | — |
| E1002 | 具名实参用了未知的形参名 | ⚠️ 零发射点 | — |
| E1003 | 同一个具名实参重复出现 | ⚠️ 零发射点 | — |
| E1004 | 同一个形参被位置实参与具名实参重复指定 | ⚠️ 零发射点 | — |
| E1005 | 实参太少：缺位形参没有默认值 | ✅ `OverloadBinder.z42:287,290,294` | `void f(int a, int b)` 调成 `f(1)` |
| E1006 | 实参太多（非 `params` 调用） | ✅ `OverloadBinder.z42:287,292` | `void f(int a)` 调成 `f(1, 2)` |

> E1005 / E1006 共用一条专门消息：**实例方法被写成静态形式调用**时，提示第一个实参是接收者。

---

## E11xx — `available!()` 宏

发射点在 [`ExprTyper.z42`](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42)。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| E1101 | `available!(X)` 的目标解析到多个重载，无法唯一确定（v1 不支持签名消歧） | ✅ `ExprTyper.z42:482` | `available!(Logger.Log)`，`Log` 有重载 |
| E1102 | `available!(...)` 的参数不是符号引用（传了字面量 / 任意表达式），或参数个数不为 1 | ✅ `ExprTyper.z42:442,463,492` | `available!(1)` |

---

## W0xxx — 警告

警告不改变退出码，只写到 stderr。驱动层的警告打印门在
[`Main.z42:435`](../../../../src/compiler/z42c.driver/src/Main.z42)；
在它打开之前，所有警告都装在诊断列表里却一个字也不会出现在终端。

| 码 | 含义 | 状态 | 触发示例 |
|---|---|---|---|
| W0603 | 包声明了保留命名空间（依赖扫描层软网） | ⚠️ 零发射点 | — |
| W0604 | 捕获的值快照被赋值 | ⚠️ 零发射点 —— 规避写法（`bool[1]` 单元格）在 stdlib 里有沿用，但编译器当前**不报**这条 | — |
| W0700 | `switch` 不穷尽：对 `bool` / `enum` / 封闭类型做 `switch` 时漏了分支，且没有 `default` | ✅ `ExhaustCheck.z42:127,154,200` | `switch (b) { case true: ... }`，`b` 是 `bool` |
| W0701 | 解构声明的绑定名遮蔽了当前类的字段 / 属性：`(A, B) = (a, b);`（花括号体里）声明的是两个**新局部**，随即离开作用域，一个成员都没动。局部遮蔽字段本身合法，单看语法挑不出毛病——只能靠「遮蔽了同名成员」这个信号拦。仅在有 `this` 的上下文里查。表达式位置的同一写法由 **E0482** 直接报错 | ✅ `StmtBinder.z42`（字面量发码；常量 `DeconstructShadowsMember`） | `class C { int A; void M(int a) { (A, _) = (a, 0); } }` |

---

## I0xxx — 信息

| 码 | 含义 | 状态 |
|---|---|---|
| I0466 | `[Forward]` 跳过某成员：外层类已自己声明了同名成员，用户的实现优先（详见上面的 `[Forward]` 小节） | ✅ `ForwardGenerator.z42:201,279`（字面量发码；常量 `ForwardSkipped`） |
| I0467 | ❌ 已退役（2026-09-22），编号不复用 —— 见 `[Forward]` 小节 |

---

## WSxxx — 工作区清单（⚠️ 整组未接线）

> 🔴 这一组码由已删除的 C# `Z42.Project.ManifestErrors` 抛出。自举后的 z42c 里**没有任何发射点**
> （`WorkspaceBuild.z42:136` 只在注释里提了一句 WS005「暂跳过」）。
> 下表保留编号登记，**这些校验当前不生效**：重复 member 名、循环依赖、policy 冲突、
> include 环 / 超深 / 路径越界、模板变量错误，工作区构建都不会拦。

| 码 | 原含义 | 状态 |
|---|---|---|
| WS001 | 两个 member 声明同一 `[project] name` | ⚠️ 零发射点 |
| WS002 | `-p` 与 `--exclude` 同时指定同一 member | ⚠️ 零发射点 |
| WS003 | Member `<name>.z42.toml` 含 `[workspace.*]` / `[policy]` / `[profile.*]` 段 | ⚠️ 零发射点 |
| WS005 | 同一 member 目录有两份 `*.z42.toml` | ⚠️ 零发射点 |
| WS006 | Member 间依赖图含环 | ⚠️ 零发射点 |
| WS007 | Manifest 在 workspace 子树内但未被 `members` 命中（原为 warning） | ⚠️ 零发射点 |
| WS010 | Member 显式声明的字段值与 workspace `[policy]` 锁定值冲突 | ⚠️ 零发射点 |
| WS011 | `[policy]` 段含未知字段路径 | ⚠️ 零发射点 |
| WS020 | Include 链含直接或间接循环 | ⚠️ 零发射点 |
| WS021 | Preset 文件含禁止段 | ⚠️ 零发射点 |
| WS022 | Include 嵌套深度超过 8 层 | ⚠️ 零发射点 |
| WS023 | Include 指向的文件不存在 | ⚠️ 零发射点 |
| WS024 | Include 路径含绝对系统路径 / URL / glob | ⚠️ 零发射点 |
| WS030 | `[workspace]` 段出现在非 `z42.workspace.toml` 文件 | ⚠️ 零发射点 |
| WS031 | `default-members` 引用了未匹配的成员 | ⚠️ 零发射点 |
| WS032 | Member 写 `xxx.workspace = true` 但根 `[workspace.project]` 未声明该字段 | ⚠️ 零发射点 |
| WS033 | `[workspace.project]` 字段类型错误 / 不可共享字段被声明 | ⚠️ 零发射点 |
| WS034 | Member 引用未在 `[workspace.dependencies]` 中声明的依赖 | ⚠️ 零发射点 |
| WS035 | 已废弃的 `version = "workspace"` 语法 | ⚠️ 零发射点 |
| WS036 | `z42.workspace.toml` 同时含 `[workspace]` 与 `[project]` | ⚠️ 零发射点 |
| WS037 | 路径模板含未知变量（含 `${env:NAME}`） | ⚠️ 零发射点 |
| WS038 | 模板嵌套 / 未闭合 / 非法字符 | ⚠️ 零发射点 |
| WS039 | 模板变量出现在不允许的字段（如 `version`） | ⚠️ 零发射点 |

> WS004 已归并入 WS010，编号不再使用。

---

## 已退役的编号空间

| 编号 | 说明 |
|---|---|
| `Z####` | 原运行期错误编号，2026-05-11 整体退役。VM 运行期错误现在通过类型化 z42 异常表达（`Std.InvalidMarshalException` 等）；catch by class 后读 `Message` / `StackTrace` 字段 |
| `E0901` / `E0902` | 见 E09xx 节 |
| `WS004` | 归并入 WS010 |
| `I0467` | 2026-09-22 退役：常量 `ForwardSkipped` 原登记此号、发射点却一直发 I0466，改值归位后空出。编号不复用（占号常量 `RetiredForwardSkipped`；`E` 前缀的 0467 是另一个码，仍可分配） |

---

## 新增一个码

1. 在 [`DiagnosticCodes.z42`](../../../../src/libraries/z42c.core/src/DiagnosticCodes.z42) 加一个码常量。
   **这是唯一能占号的地方**——`xtask test diagcodes` 不许发射任何没在这里登记过的码，于是两个并行
   PR 抢同一个号会在这个文件上产生 git 冲突（而不是双双静默合并）。
2. **加发射点**，并在提交前用 `grep -rn '"<码号>"' src/` 自证它真的会被报出——只加常量不加发射点，
   等于给了用户一条不存在的保护。⚠️ 发射点若用**字面量**（新常量与其引用不能同 PR，见
   [bootstrap-seed.md](../../../agent/rules/bootstrap-seed.md) 分阶段引入纪律），还要把
   `<码号> <相对路径>` 加进 [`scripts/test/diag-literal-emitters.txt`](../../../../scripts/test/diag-literal-emitters.txt)
   （`xtask test diagcodes --update`）。**加这一行时先停一秒**：你是不是在给一个已经有主的码挂第二个含义？
   E0474 / E0477 两次撞码正是这么来的。
3. 在本页对应分段加一行：码号 → 含义 → 状态（带 `file:line`）→ 触发示例。
   **这一步不是可选的**——规则 ⑤ 要求本页的码表与登记表双向相等，漏了就红。反过来也一样：
   想在本页「预留」或「退役」一个号，必须同时在登记表里给它一个零发射点的占号常量，
   否则门看不见它，下一个扫登记表找空位的人会把它当空号拿走。
4. 加一条回归测试，断言这个码真的被报出。

> **运行期**错误不要分配错误码：在
> [`src/libraries/z42.core/src/Exceptions/`](../../../../src/libraries/z42.core/src/Exceptions)
> 下定义一个 `Std.*Exception` 子类并抛出即可。类名 + `Message` 字段就是诊断身份，
> `StackTrace` 自动填充。
