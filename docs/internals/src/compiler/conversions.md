# 类型转换的实现

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `src/compiler/z42c.semantics/src/Conversion.z42`
> （`Classify` / `_classifyUser` / `_findConvOn`）· `TypeChecker.ConvertIfNeeded` · `TypeOpTyper._bindCastExpr`
> **相关**: [源代码编译流程](source-compile.md) · [架构总览](architecture.md) ｜ **对齐**: 2026-09-17
>
> 用户视角的**转换规则**（哪些隐式、哪些要写 `(T)`、报什么错）在参考手册的「类型转换」页
> —— 那里是语义 SoT。本页只讲**分类器怎么判、lowering 落在哪**。
>
> 本页由参考手册的 `conversions.md` 切出（三书重构批 3b）：那页带着两段「机制 / 实现」，
> 违反「reference 不写实现机制」的判据。

## 分类器的判定顺序

`Conversion.Classify` 的判定顺序（短路），镜像历史 `_isAssignable` 的分支序以保证 PR1 布尔等价：

```
1. 任一侧 error/unknown          → Absorb
2. 恰一侧泛型形参                 → GenericErase
3. to == object                  → 值 prim 源 Boxing；否则 ImplicitRef
4. 两侧数值 prim                 → 数值矩阵（Identity / ImplicitNumeric / ExplicitNumeric）
5. from.IsAssignableTo(to)       → Identity（同名类 / 接口 / 数组 / func / 别名 prim）
6. class/instantiated → 基/接口  → 命中 symbols 上转查询则 ImplicitRef；下转则 ExplicitRef；否则 None
   6a. instantiated → 接口（G）  6b. instantiated → 裸 class（H）
   6c. instantiated → instantiated（H2）  6d. 裸 class → instantiated（I）
   6e. interface → 祖先 interface（F2）
7. object/接口 → 值 prim         → Unboxing
8. 否则                          → None
```

> **步 6e（F2）也是后补的**（`interface-assignability`，2026-09-26），形状与 H2 一模一样：
> class→iface（F）、inst→iface（G）、class→class（E）都在，**独缺 iface→iface** ⇒
> `IBase b = derivedIface` 落到步 8 报 E0402。同一个洞在五个消费点各露一次面（赋值 / 实参 /
> 返回 / 函数位约束 `ConstraintChecker._funcPosOk` / 接口实现的返回协变
> `InheritanceResolver`），现已全部收敛到 `InterfaceClosure.IsInterfaceSubtype` 这**一个出口**。
>
> 判定走 `InterfaceClosure.BaseAt` 沿父接口链上溯，**不走** `SymbolTable.InterfaceDerivesFrom`：
> 后者比的是 `BaseNames` 里的字符串，既认不出两种拼写（`: IDisposable` vs `: Std.IDisposable`），
> 也在裸名化时把链上的类型实参丢了。`BaseAt` 这两件事本来就做对了（解析声明形态 + 逐层代换
> 实参），所以这条边是搭在既有机制上的，没有第二套遍历。
>
> ⚠️ 同一次变更修掉了**步 5 的对称缺陷**：`Z42InterfaceType.IsAssignableTo` 此前只比 `Name()`，
> 而 `Z42InstantiatedInterfaceType.Name()` **刻意返回裸名**（元数据拼写要稳）⇒ `IBox<int>` 与
> `IBox<string>` 在步 5 就被判成 `Identity`。这与 H2 那条「只比 `Name()` 全等」是同一个坑的
> 接口版，但后果更重：类那半是**误报**（编不过、拦住正确代码），接口这半是**静默错值** ——
> 实测 `IBox<string> s; IBox<int> i = s; int bad = i.Get();` 编译期零诊断，跑出 `bad = hello`、
> `bad + 1 = hello1`，exit 0。接口身份的判据现已收敛到 `Z42InterfaceType.SameInterface`
> （两侧 ns 齐备时比 FQ、否则比短名；再逐位比类型实参的规范名。一侧是**裸定义**时按名放行 ——
> 那表示实参未知，与本仓「信息不足一律放行」的口径一致）。
>
> **`Name()` 保持裸名不变**：它同时是元数据拼写与查找键，改它会打烂派发与 golden。诊断文本
> 另走 `Z42Type.DiagName`（实例化接口 → `NameWithArgs()`，其余原样），否则实参不符会打印成
> 「cannot assign IBox to IBox」。也刻意**不改 `Dump()`** —— 那是 `--dump-bound` 的既有形态。

> **步 6c（H2）是后补的**（`fix-binder-emitter-gaps-batch2`，欠债表 bug B）。G/H/I 三条早就在，
> **独缺 inst→inst**，而步 5 的 `Z42InstantiatedType.IsAssignableTo` 只比 `Name()` 全等 ⇒
> `Bag<int> b = new SubBag<int>();` 直接 E0402。H2 的判定 = **类型实参逐位规范同名**
> （C# 类不变量：`Bag<string> b = subBagOfInt` 必须继续报错）**且** `Def` 名有子类关系。
>
> ⚠️ H2 只有在**基类链本身可走**时才有意义。同一次变更修掉了更深的一层：`Z42ClassType.BaseName`
> 此前存的是**带泛型实参的基类文本**（`"Bag<T>"`），而它的每个消费方都拿它当 `Classes` 的键用
> ⇒ base 链在泛型基类处**静默截断**（`IsSubclassOf` 恒 false、继承成员找不到）。详见
> source-compile.md「基类名裸名化」。
>
> **未覆盖**：基类声明处换了实参（`class Sub<T> : Bag<string>`）。`BaseName` 只存名字、不存基类
> **实参**，无从代换 ⇒ 仍报 E0402（与修前同，无回归）。同因 `GBase<int> b = new CSub();`
> （非泛型派生 → 泛型基类实例化，步 6d 要求 `Def` 同名）也仍不通。要修得正确需在类符号上存
> 「已解析的基类型」而非基类**名字**，属类型模型改动。

> **关键设计**：数值 prim 对（步 4）**提前到结构判定（步 5）之前**——否则有损拓宽（`int→float`）
> 会被 `IsAssignableTo`（其 `_canWiden` 判其为拓宽）笼统当成 `Identity`，丢掉"有损"信息。提前后
> 数值对一律走细粒度矩阵。这不改 PR1 的布尔投影（数值对无论哪种都落在宽松门白名单内），只让
> **种类标签正确**，为 PR2 的收紧提供准确依据。

`_isAssignable(from, to, symbols)` = `Classify(...).ImplicitOk()`（PR2 收紧门）。它是**纯类型**判定
（不看表达式），窄化 / 有损浮点返回 `false`；重载候选决议等复用它的地方，窄化实参因此不再"可赋"
= 不参与该候选（与 C# 一致）。

### 隐式上下文检查：`CheckImplicitConvert`（含常量在范围内例外）

赋值 / return / 传参这些**隐式上下文**的检查经 `TypeChecker.CheckImplicitConvert(value, target, …)`——
比纯类型 `_isAssignable` 多一层**表达式感知**：

```
1. Classify(value.Type(), target).ImplicitOk()  → true（放行）
2. ExplicitNumeric ∧ 目标整数/char ∧ value 是编译期常量整数且在目标范围内 → true
      （C# 常量在范围内例外：`byte b = 48;` ✓，`byte b = 300;` ✗）
3. 存在显式转换（Exists）→ 报 E0439「cannot implicitly convert 'X' to 'Y';
      an explicit conversion exists (are you missing a cast?)」
4. 否则（根本无转换）→ 报 E0402 TypeMismatch
```

> **常量例外**只覆盖**整数/char 目标**（`_constIntInRange`，复用编译器权威 `ZbcInstr._parseIntLit`）：
> 在范围内的常量窄化**逐值可证无损**，与「隐式只允许绝对无损」一致，且令 binary-format writer
> 里 `bytes[i] = 48;` 这类免于满屏 `(byte)`。**有损浮点无此例外**（`float f = 5;` 仍要 `(float)`）。
> 常量**表达式**折叠（`byte b = 40 + 8;`）超出 PR2 覆盖面（当前仅字面量 / 一元负号字面量）。

### 数值拓宽插 `ConvertInstr`：`ConvertIfNeeded`

隐式**拓宽**（`int→double`、`char→int` 等）历史上**不发** `ConvertInstr`——所有整数运行期同为
`Value::I64`，`double d = 5` 会把 `I64(5)` 存进 F64 槽（表示 bug）。`TypeChecker.ConvertIfNeeded`
在每个协变点（return / var-decl / assign / call-arg，镜像 `BoxIfNeeded`）当**运行期表示类**变化时
包 `BoundConvert` → codegen 发 `ConvertInstr`：

| from→to | 表示类 | 插 `ConvertInstr`? |
|---------|--------|:---:|
| `int→long`、`byte→int` | INT→INT（运行期同 `I64`）| ✗ no-op |
| `int→double`、`uint→double` | INT→FLOAT（`I64→F64`）| ✓ |
| `f32→f64` | FLOAT→FLOAT（运行期同 `F64`）| ✗ no-op |
| `char→int`、`char→double` | CHAR→其它 | ✓ |

> 只在表示类真变化时插——等宽整数拓宽与 `f32↔f64` 是 no-op，最小化字节扰动（z42c 自身源码不含
> 隐式 int↔float 拓宽 → 其 codegen 逐字节不变，自举不破代）。副作用：`Math.Pow(2,3)` 的 int 实参
> 现正确拓宽为 `F64` → Pow 遵守其 `double` 签名返 `F64(8.0)`（此前因 native 的 `(I64,I64)→I64`
> 分支静默返 `Int32(8)`）。


---

## 用户自定义转换的实现


| 环节 | 落点 | 说明 |
|------|------|------|
| 关键字 | `TokenKind.z42` `Implicit`/`Explicit` + `Lexer._initKeywords` | `implicit`/`explicit` 成保留字（support 先行，z42c/stdlib 晚一个 nightly 才用） |
| 解析 | `MemberParser._parseMemberBody` | `implicit/explicit operator Target(Source s)` → 方法 `op_Implicit`/`op_Explicit`（静态、单参、返回=Target） |
| `(T)x` 消歧 | `ExprParser._castOperandStart` | `(Ident)operand`（operand 起于标识符/字面量/new）解析为 `CastExpr`；`(a)-b`/`(f)(x)` 仍按二元/调用 |
| 分类 | `Conversion._classifyUser` / `_findConvOn` | 内建转换 `None` 时回退：在 from 类与 to 类的 `Methods` 上找 op_Implicit/op_Explicit（精确 (源,目标) 匹配）→ `ConvResult{UserImplicit\|UserExplicit, Method}` |
| lowering（隐式） | `TypeChecker.ConvertIfNeeded(_,_,syms)` | UserImplicit → `_lowerUserConv` 包成静态 `BoundCall`（op_Implicit）；已过 `CheckImplicitConvert`（UserImplicit 在 `ImplicitOk` 白名单） |
| lowering（显式） | `TypeOpTyper._bindCastExpr` | UserImplicit/UserExplicit → `BoundCall`；数值/引用 cast 仍 `BoundConvert` |
| 无格式 bump | — | 全部复用既有 Call opcode（同 `op_Add` 脱糖），无新 IR、不 bump zbc/zpkg |

**RegKey 唯一（根因修复）**：静态方法仅按参数类型 mangle（`op_Implicit$1$Foo`），两个同源不同目标的转换
（`operator int(Foo)` + `operator string(Foo)`）会撞键。转换运算符 RegKey 附返回类型消歧为
`op_Implicit$1$Foo$to$i32`（`SymbolCollector` `_isConvOp` 分支）——RegKey 是 body 绑定 / IrGen / 派发的
单一真相源，一处改全链一致。

