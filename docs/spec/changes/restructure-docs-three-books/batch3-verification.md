# 批 3 字段级核实报告

> 沿用批 1 / 批 2 建立的方法：**grep 命中 ≠ 描述准确**，必须落到字段 / 函数 / 语法级。
> 本批额外用 `./.z42/z42 run` 写探针**实跑**验证——这是批 1/2 没做、而本批收益最大的一步。

## 一、核实规模与命中率

| 组 | 抽查断言 | 命中 | 半命中 | 落空 |
|---|---:|---:|---:|---:|
| 合并对（properties / iteration / strings / access-control） | 23 | 9 | 2 | 12 |
| internals 切片（interop / object-protocol / boxing / closure / attributes / generics） | 28 | 11 | 7 | 10 |
| `language-overview` 九页拆分 | 45 | 21 | 8 | 16 |
| 直迁 11 篇 | 48 | 19 | 7 | 22 |
| **合计** | **144** | **60** | **24** | **60** |

**落空率 42%。** 集中在两类：

1. **自举把 C# 侧机制整族带走了** —— interop 的 manifest 全族（`NativeImportSynthesizer` /
   `ManifestSignatureParser` / `NativeManifest`）、closure 的 `ClosureEscapeAnalyzer` /
   `TypeEnv._funcAliases`、`Z0901`–`Z0904` 码族、`GrammarSyncTests` parity 门禁。
2. **运行时表示重构过两轮而设计文档没跟** —— `Value::Boxed(Box<BoxedPrim>)` → `BoxedStruct`
   （判别号 13 已留空）、四个瞬态变体改成 arena 句柄、`VmContext.env_arena_stack` 合并进统一帧向量。

## 二、推翻搬迁清单的裁决

见 [tasks.md](tasks.md) 批 3 节的「裁决」表（A–G 七条）。核心一条：

> **清单的「主干判定」是按文件名和新旧程度猜的，不可照搬。** 批 2 已推翻 7 对中的 2 对，
> 批 3 推翻的比例更高——`iteration` / `foreach` **两份都不是主干**（真实现是三-path 决策树，
> 两份各只描述了其中一部分且都有误），只能照源码重写。

## 三、可操作的通用规则（建议写进清单，后续批次别再逐份核实）

1. **凡 `docs/design/` 里带「Phase 1 / Phase 2」表格或「限制表」的小节，默认判为过期。**
   本批 4 组里 4 处 Phase/限制表**无一例外全部作废**。
2. **凡表格列有「实现位置 / Pipeline」且指向 `.cs` 路径的，一律作废。**
   筛法：`grep -rln 'z42\.Syntax\|\.cs::\|\.cs)' docs/design/`
3. **凡文档声称「编译器会报 Exxxx」的，要么给得出发射点 `file:line`，要么标注「未实现」。**
   本批发现大量「定义了但零发射点」的死码（见附录 B），被文档写成生效规则会误导用户以为有保护。

---

## 附录 A：实测发现的实现缺口（30 条）

> **全部用 `./.z42/z42 run` 实跑验证，不是静态推断。** 这些超出文档批次范围，建议单开 change。
> 按严重度排序——**前 4 条都是「编译通过但行为静默错误」，无任何诊断**。

### A1 🔴 静默数据丢失（4 条，同一族）

| # | 写法 | 应当 | 实测 | 根因 |
|---|---|---|---|---|
| 1 | `void Inc(ref int x)` 调用写成 `Inc(v)`（漏写 `ref`） | 编译报错 | **编译通过，写入静默丢失** | 调用点修饰符不校验 |
| 2 | `Inc(ref arr[0])` | `arr[0]` 被改 | **写入静默丢失**（打印 10 不是 11） | `z42.ir` 只有 `LoadLocalAddrInstr`，**没有 `LoadElemAddrInstr` / `LoadFieldAddrInstr`**；`ExprEmitter.z42:110-123` 对任何 `BoundRefArg` 一律「先发射 inner 再取该**临时寄存器**的地址」。运行时侧 `RefKind::Array` / `Field` 有定义但**无生产者** |
| 3 | `Inc(ref h.f)` | `h.f` 被改 | **写入静默丢失**（打印 20 不是 21） | 同上 |
| 4 | 单字段 `struct P { int x; }` 的 `var b = a; b.x = 99;` | `a` 不变（值语义） | **`a` 也变**（引用语义） | `StructLayout.IsBlobStruct` 要求「**多字段**且各字段非嵌套 struct」，单字段 struct 走不到 blob 值语义路径。两字段及以上（局部变量、数组元素）实测值语义正确 |

### A2 `ref` / `out` / `in` 三者塌缩（5 条）

根因：`src/libraries/z42c.syntax/src/MemberParser.z42:340` ——
`if (Kind == Ref || Kind == Out || Kind == In) { _advance(); isRef = true; }`，
三个修饰符在 AST 上塌成同一个 `Param.IsRef` 布尔（`Decl.z42:12`）。
硬证：`src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42:99` 注释原文
「callee 的 IR 完全看不见 `ref` 修饰（`Param.IsRef` 只影响 caller 侧发 `load_local_addr`）」。

| # | 规则 | 实测 |
|---|---|---|
| 5 | `in` 只读 | `void M(in int x) { x = 99; }` 编译通过，调用方变量被改成 99 |
| 6 | `out` 必须赋值（明确赋值分析） | `bool M(out int v) { return false; }` 编译通过；调用方读 `v` 抛 `__box_prim: expected integer value, got Null`。`grep -i "definiteassign\|unassigned"` 在 semantics + DiagnosticCodes **零命中** |
| 7 | 实参必须是 lvalue | `Inc(ref 42)` / `Inc(ref f())` 都编译通过 |
| 8 | 跨修饰符类型严格匹配 | `long n; Foo(ref n)` 匹配 `Foo(ref int)` 编译通过，且写回照常生效 |
| 9 | 修饰符参与重载 | **不参与**。`OverloadResolver.z42:92` 的 `MangleKey(name, paramTypes, paramCount)` 不含修饰符位，`Foo(int)` 与 `Foo(ref int)` 撞同一个键 → `E0408: duplicate overload` |

（另：`void Foo(ref int x) { var f = () => x + 1; }` 编译通过——lambda 捕获 `ref` 形参未禁止。）

### A3 其它（7 条）

| # | 缺口 | 证据 |
|---|---|---|
| 10 | `arr[f()] += 1` 把 `f()` **求值两次** | `AssignTyper.z42:129/138/141` 同一个 `target` 对象既做 `BoundAssign` 目标又做 `BoundBinary` 左操作数；`AccessEmitter._emitAssign` 无去重。测试只覆盖了常量下标 |
| 11 | `Dictionary<K,V>` 直接 `foreach` 会误入索引路径 | 它同时有 `Count` 字段和 `this[TKey]` 索引器 ⇒ 命中 `StmtBinder.z42:32-33` 的索引 duck-typing，拿 **int** 下标去调 `get_Item(TKey)`。正解是 `dict.Keys()` / `dict.Entries()` |
| 12 | `new int[n][]` **不解析** | `_parseType` 只吃紧挨的 `[]`，`[n]` 之后的 `[]` 变成坏下标。C# 里「按运行期长度建外层行数组」在 z42 写不出来——`Aes.z42` / `BigInt.z42` / `ZpkgWriter.z42` / `BundleRunner.z42` 里那几条「z42 不支持 jagged」的绕行注释即由此而来（**注释本身已过期**：jagged 字面量与索引实测全部支持）。替代写法 `int[][] rows = [null; n];` 实测通过 |
| 13 | 闭包栈分配**已失效** | `MkClosInstr.StackAlloc` 在编译器三个发射点全写死 `false`（`FunctionEmitter.z42:485` / `ExprEmitter.z42:346` / `CallEmitter.z42:504`）；`ClosureEscapeAnalyzer` 已不存在。运行时侧实现仍完整挂着。⚠️ `internals/src/runtime/vm-architecture.md:744` 与 `escape-analysis.md:11` 两处仍在说它生效 |
| 14 | `foreach` 迭代变量只读**未强制** | 无对应检查 |
| 15 | 字符串插值的格式说明符 `$"{x:X2}"` **被静默丢弃** | `ExprParser.z42:806-809` 把洞内文本丢给 `Parser.ParseExpression()`，而它（`Parser.z42:215`）**不校验消费到 EOF** ⇒ `:X2` 既不报错也不生效 |
| 16 | 数组越界是**非对象抛出** | `catch { }` 能接住，`catch (Exception e)` **接不住**（`exec_array.rs:196` `bail!`） |
| 17 | `int x; x += 2.5;` **编译通过、运行期才 trap** | `x = x + 2.5` 正确报 `E0439`，但复合赋值不报。`AssignTyper.z42:~136` 把 desugar 出的 `BoundBinary` 硬标成 `target.Type()`，后续 `_finishValue` 看到 int→int 就放行 ⇒ 运行期 `__box_prim: expected integer value, got F64` |
| 18 | switch **表达式**无匹配 arm 时**静默返回 `null`** | 实测。不抛异常、不报警告 |
| 19 | 调用 null delegate 不是可 catch 的异常 | `exec_call.rs:371` 是 `bail!` 而非 `NullReferenceException`（未走 make-corelib-errors-catchable 那条路）⇒ 必须用 `?.Invoke()` |
| 20 | `[Forward]` 四个诊断码的**常量名与实际发射的语义全部错位** | `ForwardTargetNotFound = E0464` 实际发的是「渲染不出签名」；`ForwardNotRenderable = E0465` 实际发的是「不可转发或不唯一」；`ForwardAmbiguous = E0466` 零发射点（重载歧义实际走 E0465）；`ForwardSkipped = I0467` 零发射点（跳过实际发 `I0466`）。⚠️ **照常量名写文档会全篇写反** |

### A4 类型系统缺口（10 条，全部实跑验证）

| # | 缺口 | 实测 |
|---|---|---|
| 21 | 🔴 **struct 的 `static` / `static readonly` 字段是坏的** | 读取时抛 `struct-value handle used after its creating frame exited — value-struct lifetime unsound`。加不加 `readonly` 都一样。⇒ `public static readonly Color White = ...` 这种惯用法在 z42 用不了，只能改用静态工厂方法 |
| 22 | 🔴 **struct 上的自动属性是坏的** | `public int X { get; set; }` + 构造器赋值 → 运行期 `struct ref leaf at byte offset 4294967295 not in type layout`（`4294967295` = `u32::MAX`，即未初始化的 offset） |
| 23 | 🔴 **重载集里不做「子类 → 用户基类 / 接口」适用性匹配** | 单签名 `One(A)` 传子类 `D` ✅；但有 `F(A)` / `F(string)` 两个重载时传 `D` → `E0401: no static method F on C`（实例方法同样）。数值加宽与 `object` 形参在重载集里**是**适用的，唯独用户类层次不是 |
| 24 | **接口不能继承接口** | `interface IDerived : IBase` 解析通过但不生效：`d.Base()` → E0401，`IBase b = d;` → E0402 |
| 25 | **接口方法体不是默认实现** | 写了 body 仍报 `E0412: does not define member Hello` |
| 26 | **自定义 `static abstract` 接口在运行期崩** | `INumber` 是 `BuiltinTypeDefs.z42:81` **硬编码的内建接口**。自己声明同形接口 `interface ICombine { static abstract Self op_Add(Self a, Self b); }` → 运行期 `MissingSymbolException`（`Cnt.op_Add` 签名 3 参 vs 调用 2 参）。另 `T.Zero()` 形态 → `E0401: undefined: T` |
| 27 | `Object.ReferenceEquals` **用户代码调不到** | `Object.ReferenceEquals(a,b)` 与 `Std.Object.ReferenceEquals(a,b)` 均报 `E0401: no static method 'ReferenceEquals' on 'Object'`；全仓零调用点（只有 `DelegateOps.ReferenceEquals` 在用） |
| 28 | **默认值参数顺序不强制** | `F(int a = 1, int b)` 能编译 |
| 29 | **函数值不能就地调用** | `bus.Handlers[0](9)` → `E0402: unsupported call form`；`var h = bus.Handlers[0]` → `E0401: undefined function: h`。必须先赋给写明 `(T) -> R` 类型的局部变量 |
| 30 | `default(自定义 struct)` 随即崩 | `StructCopy src: expected a struct value (StructRef), got Null`（与 A3 #17 的 `Guid` 注释同源） |

另：表达式体构造器 `public Pair(int a, int b) => (A, B) = (a, b);` **编译通过但字段全是 0** ——
`(A, B) = (...)` 走的是解构**声明**（`_parseDeconstructDecl`），声明了两个新局部而不是赋值给字段。

## 附录 B：已定义但零发射点的诊断码

> 这些码被文档写成生效规则，实际**永远不会被报出**。搬迁时已逐条改标「未实现」。

| 码 | 常量名 | 文档声称 | 实际 |
|---|---|---|---|
| `E0424` | `IllegalCast` | 非法 cast 报错 | 零发射点。`(int)true` 在编译期**静默放行**（`TypeOpTyper.z42:17-38`）；实际走 E0402 / E0439 或运行时 `InvalidCastException` |
| `E0420` | `InvalidCatchType` | catch 类型须派生自 `Std.Exception` | 零发射点。`catch (Foo e)`（Foo 非 Exception 子类）**不报错**（`StmtBinder.z42:174-209` 只做可见性检查） |
| `E0414` | — | event 字段访问控制 | 零发射点。`EventFields` 收集于 `MemberCollector.z42:184`，**TypeChecker 从不查询** |
| `E0602` | — | 未解析的 using | 零发射点（除测试里的字符串断言） |
| `E1001`–`E1004` | 具名实参四码 | 已启用 | 零发射点。`OverloadBinder.z42:287-294` 只发 **E1005 / E1006** |
| `E0605` | `ReservedNamespaceDeclaration` | 「源码层硬错误」 | 零发射点 |
| `W0603` / `W0604` / `W0700` | — | 警告 | `z42c.driver/src/Main.z42:428-431` 注释自述这些警告「**一直是哑的**」 |
| `E0422` / `E0423` | func 类型约束 | 已定义 | 从未发出（roadmap 已记） |

**补齐后的实测口径**（`reference/src/appendix/error-codes.md` 已从 38 条补到 115 条）：

- ✅ 生效（带发射点 `file:line`）：**81 条**
- ⚠️ 已定义未接线（零发射点）：**54 条** —— 编译器码 31 + `WSxxx` 整组 23
- ❌ 已退役：`E0901` / `E0902`（连常量定义都不存在，`git log -S` 零命中；随 C# 编译器一并消失）

`WSxxx` 整组 23 条来自已删的 `Z42.Project.ManifestErrors`，自举时未移植；旧文档按
C1/C2/C3/C4a 分节标「已启用」。原生互操作 `E0903`–`E0916` 整组同理。

⚠️ 一处此前的判断需更正：`W0700`（switch 不穷尽）**是活码**，不是死码 ——
`ExhaustCheck.z42:127,154,200` 真发（bool / enum / 封闭类型三种），
`Main.z42:428-431` 那条「一直是哑的」注释是**历史陈述**，紧随其后的 `:435-446`
已把 warning 打印门打开。`W0603` / `W0604` 仍是死码，但原因是**没有发射点**，不是门哑。

另：旧页头声称的 `z42c explain <code>` / `z42c errors` 两个子命令**不存在**。

## 附录 C：过程教训

1. **同一个 worktree 里有 subagent 在写文件时，不要 `git commit`**（`git add -A` 更不行）。
   本批犯了两次，内容没丢但产出散在「commit + 工作区」两处，不好拆分提交。
   ⇒ 规矩：**所有并行任务报完再提交**。
2. **章程自身的歧义会传染。** `doc-system.md` §2.1 的箭头图 `learn → reference → internals`
   与紧接着的散文「**reference 不得链 internals**」自相矛盾，导致本批一度按图给出错误指令。
   已改成明确写法（只改表述，不改规则）。
3. **转述二手结论会失真。** 本批把「stdlib 在用 `byte[][]`」当作 jagged 已支持的证据下发，
   实际那几处全是**写着「不支持」的绕行注释**。执行方没照抄、而是实跑定性，才发现真缺口是
   `new int[n][]` 不解析。⇒ **证据要给出处，执行方有义务复核。**

---

## 附录 D：批 3b 的核实结果

批 3b（六篇切片 + `generics.md` 瘦身）沿用同一方法，又推翻了执行简报的多条判定，
并挖出 3 条用户可见的实现缺口。

### D1 简报被推翻的 7 条（interop 族）

| 简报写的 | 源码实情 |
|---|---|
| `#[derive(Z42Type)]` 示例可用，划给 reference | **是 `compile_error!` 占位**（`z42-macros/src/lib.rs:22-37`）—— 照搬即向用户发一份编译不过的例子 |
| Rust↔ABI 映射表可用 | 大半不成立：`&T` / `&str` / `String` / `Vec<T>` / `&[T]` / `Box<T>` / `Result<T,E>` / 按值 `self` 全被 `signature.rs::parse_type` 拒绝 |
| `pinned` 语法语义 → reference | **整条链不存在**：只有词法关键字 `TokenKind.Pinned`，无 AST / 解析 / 类型检查 / IR；`z42.ir` 里连 `PinPtr` 指令类都没有 |
| `E0903` / `E0904` 现存 | **也是零发射点死码**（只有 `DiagnosticCodes.z42:127-128` 两行常量），与 `E0907/E0909/E0916` 同族 |
| `[Layout]` / `[FieldOffset]` / `[UnmanagedCallback]` 成节 | 全仓零命中，从未实现 |
| （未提及） | **`[Native(lib=,type=,entry=)]` 才是活的核心用户契约**（`StubEmitter.z42:79-84` 按有无 `type=` 分流 `CallNativeInstr` / `BuiltinInstr`）；且 `lib=` 在 builtin 那条路上**完全不参与解析** |
| §7.1「JIT 发直接 call」 | JIT **不支持** `CallNative`/`CallNativeVtable`/`PinPtr`/`UnpinPtr`（`jit/translate/unsupported.rs:42-43`），含这些指令的函数整体回落解释执行 |

### D2 两次「拒绝新建页」——判断正确，避免了第二份 SoT

| 我的指令 | 执行方的反驳 |
|---|---|
| 为 `object-protocol` 在 reference 新建语义契约页 | `reference/language/classes.md:52-88` **已完整覆盖**四方法表、「覆写 `Equals` 必须同时覆写 `GetHashCode`」、struct 不继承 Object、`Type` 描述符 ⇒ 新建即第二份 SoT |
| 把 `closure.md` §3.1–3.4 语法搬进 reference | `reference/language/functions.md:236-296` 已覆盖且**更准**（已记「函数值必须先落到写明函数类型的局部变量才能调用」这条限制）|

⇒ **教训：派活前先查目标书已经写了什么。** 简报是按源文档的目录列的，没查接收方。

### D3 新挖出的 3 条实现缺口（实跑验证）

| # | 现象 | 根因 |
|---|---|---|
| 31 | 🔴 **`"..." + obj` 绕过用户的 `override ToString()`**，而 `$"{obj}"` 不绕过 | `exec_value.rs:58-59` 的 `Add` 混合臂直接调 `value_to_str`（不查 vtable）；emitter 侧 `OperatorEmitter.z42:47-56` 的 `+` **不发 `ToStr`**。源文档明说「`+` with a string operand → implicit ToStr」，是反的。基元 / enum / 数组不受影响 |
| 32 | 🔴 **`f == f` 返回 `false`**（同一函数两次取引用也是 false）| `__delegate_eq` builtin 存在且有单测，但**编译器侧零发射点** —— `==` 从没接到委托相等语义上。正确写法是 `DelegateOps.ReferenceEquals` |
| 33 | **`f += g` 不是编译错误** | 编译得过，运行期才炸 `type mismatch in arithmetic: FuncRef(...) vs FuncRef(...)`。源文档标的「❌ 编译错误」不成立 |

### D4 顺带修正的既有文档错误

- **装箱插入点从 5 处扩到 9 处**：新增**再赋值**（`AssignTyper.z42:153` —— 源码注释**自陈**
  `BoxIfNeeded` 头注「宣称覆盖赋值」而实际**根本没有装箱点**）、索引器 set、泛型方法实参、
  record 合成 `GetHashCode`。后三处共因：手搭 `BoundCall` 绕过了 `_withDefaults`→`BoxArgs` 汇聚点
- **`__box_prim` 只覆盖整数与 enum**；bool / char / float / double / string **不装箱**（各有 `Value` 变体）。
  源文档的 `BoxedPrim{ inner = 裸基元值(I64/F64/Bool/Char/Str) }` 是错的
- **基元的 `__int32_equals` / `__int32_hash_code` / `__double_*` / `__char_*` 全部已删**
  （`shrink-primitive-native-interop` Stage 2）⇒「基元 Equals/GetHashCode 走 hardcoded builtin」整条作废
- **`unify-vcall-resolution`（2026-09-03）已把 interp/JIT 两份 ~900 行阶梯合并**成
  `interp/vcall_resolve.rs` ⇒ 源文档的「三条路径」框架废弃，新页按现行**四级接收者阶梯**重写
- **enum 精度边界整节已过期**：`make-enum-distinct-type` 1.5 让 enum 走 `__box_prim` 但盒带
  **enum 自身**的 type_desc ⇒ Deferred `add-boxing-future-enum-precise` **已完成**，roadmap 该行已划掉
- **`ref` 形参可被 lambda 捕获**（源文档说不可）—— 捕获到的是值快照
- 闭包栈分配三个发射点行号订正：`FunctionEmitter.z42:490`（非 485）、`ExprEmitter.z42:356`（非 346）
