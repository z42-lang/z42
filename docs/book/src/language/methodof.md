# `methodof` —— 方法引用表达式

> 与 [`typeof(T)`](#与-typeof-的对称性) 对称：`typeof` 指代一个**类型**，`methodof` 指代一个**方法**。
> 求值为 `Std.Reflection.MethodInfo`。

## 它解决什么

在此之前，z42 没有任何**在代码里精确指代一个方法**的手段。想拿到一个 `MethodInfo`，只能
`typeof(X).GetMethods()` 再按字符串名筛——重载还筛不开：

```z42
// 之前：手写循环按名字筛，两个 Log 重载根本分不开
MethodInfo find(Type t, string name) {
    MethodInfo[] ms = t.GetMethods();
    int i = 0;
    while (i < ms.Length) { if (ms[i].Name == name) { return ms[i]; } i = i + 1; }
    return null;
}
```

痛点在 attribute 场景最尖锐——想让 attribute 记录「这个声明关联哪个函数」（路由表、事件
注册、序列化字段选择器、测试桩），此前只能塞字符串：

| | `[Route("HandleGet")]` 字符串 | `[Route(methodof(Api.HandleGet(string)))]` |
|---|---|---|
| 方法不存在 | 运行期才发现，或**静默找不到** | **编译错误** |
| 方法被重命名 | **静默失效**，无任何提示 | **编译错误**，引用点立刻红 |
| 重载 | 指不明 | 参数类型列表精确选中 |
| 签名漂移（参数改了） | 运行期 `Invoke` 才炸 | 编译期匹配不上即报错 |

一句话：**把一个运行期的、静默的失效模式，变成编译期的、必然暴露的错误。**

## 语法

```z42
methodof(Logger.Solo)                  // 无重载时可省略参数列表
methodof(Logger.Log(string))           // 参数类型列表消歧
methodof(Logger.Log(List<int>, int))   // 泛型实参零歧义（见下）
methodof(Logger.Log())                 // 显式指代**零参**那个重载
methodof(Logger.get_Level)             // 属性访问器按方法名指代
methodof(Demo.Api.Handle)              // owner 可以是限定名
```

### 括号内是签名语法，不是表达式语法

这是本设计的技术关键。在表达式文法里 `Logger.Log(List<int>)` 的 `List<int>` 与 `<` 比较
运算**真歧义**；而 `methodof(...)` 的括号是一个封闭作用域，里面每个 `<` 都是类型实参，歧义
消失。语法是**局部的**，不污染表达式文法。

> Eric Lippert 当年吐槽 C# 的 `infoof(Bar(int,int))` 是「你引入了一种新语法——C# 里从来没有
> 过括号包起来、逗号分隔的类型列表」。对 C# 是负担；对 z42 不是，因为它封闭在 `methodof`
> 的括号内，别处一概看不见。

### 省略参数列表：仅当候选唯一

`methodof(X.M)` 是一等写法，合法**当且仅当** `M` 在候选集里唯一。

- **候选集含继承链与跨包 imported 成员**——基类有同名方法即算重载，不能只看本类。
- 候选 ≥ 2 时**报错，绝不静默选一个**。静默择一会把本特性要根治的「静默失效」原样搬回来。
- `methodof(X.M)`（省略）与 `methodof(X.M())`（显式零参重载）**语义不同**。

**已知代价（预期行为，非缺陷）**：给目标方法**新增一个重载**，会让所有已写的 `methodof(X.M)`
从合法变成编译错误。这与本特性的目标一致——签名面变化就该在引用点暴露，而不是静默改绑到
另一个重载。诊断会直接给出补签名的写法。

## 与 `typeof` 的对称性

| | `typeof(T)` | `methodof(X.M(…))` |
|---|---|---|
| 指代 | 类型 | 方法 |
| 结果类型 | `Std.Type` | `Std.Reflection.MethodInfo` |
| 解析时机 | 编译期 | 编译期（重载决议全部在绑定期完成） |
| 位置限制 | 无 | 无（attribute 实参、普通代码一视同仁） |
| 对象身份 | 每次求值**新建**，`typeof(T) == typeof(T)` 为 `false` | 同左，`methodof(X.M) == methodof(X.M)` 为 `false` |

对象身份那条是刻意保持对称的：单给 `methodof` 加驻留缓存会让两个号称对称的特性行为不
一致，并悄悄引入对象身份语义。反射对象驻留是独立的优化项，要做就两边一起做。

## attribute 里的 `methodof`——为什么 z42 能做而 C# / Java 不能

```z42
class HandlerAttribute : Attribute {
    public MethodInfo Target;
    public HandlerAttribute(MethodInfo target) { this.Target = target; }
}

[Handler(methodof(Api.HandleGet(string)))]
class GetRoute { }

[Handler(methodof(Api.HandleGet(int)))]      // 同名不同签名，分别精确指代
class GetByIdRoute { }
```

C# 和 Java 都否决过这个特性，**原因不在语言、在元数据格式**：

- ECMA-335 II.23.3 的 CustomAttrib blob 只允许基元 / string / `System.Type` / object /
  装箱值类型 / 一维数组，**没有任何方法引用编码**——连 `typeof(T)` 在 blob 里都只是一个
  类型全名字符串。
- JVM 的 `element_value` tag 集合同样没有方法引用。
- 两个平台的**字节码**都能表示方法常量（IL 的 `ldtoken`、JVM 的 `CONSTANT_MethodHandle_info`），
  缺口纯粹在注解格式，而格式在方法常量出现之前就冻结了。
  （csharplang 的官方口径：「supported types … are limited by the runtime … and how those
  types are encoded into the metadata」；`infoof`/`methodof` 提案已 CLOSED。）

**z42 不受这个约束**：z42 的 attribute 不走常量 blob，走**工厂函数**——编译器把 attribute 的
实参 AST 原样塞进一个合成的 `__attr$…()` 函数体里编译，元数据里只存 `(attrTypeName,
factoryFuncName)` 两个字符串，运行时 `GetCustomAttributes` 调该函数把对象造出来。
attribute 实参本来就是**任意表达式**，没有常量性约束。

⇒ `methodof` 只需要是一个合法表达式，**元数据侧零工作、零格式 bump**。

工厂函数合成的机制细节见 [源代码编译流程（z42c）](../compiler/source-compile.md)。

## 稳定性 = 普通调用的稳定性

`methodof` 在**编译期**就把目标解析成一个 qualified 名（`<声明类 FQN>.<派发键>`），落到 IR
时只剩**一个字符串池索引**——与一次普通 `Call` 指令面对完全相同的敞口，不引入新的脆弱性。
同一方法被 `methodof` 引用 N 次 = 池里 1 条 + N 个 u32，体积与一次普通函数调用等同。

派发键本身就编码了重载身份（`Name` / `Name$arity` / `Name$arity$typesig`），所以 `methodof`
**没有另造一套签名编码**——它发出的就是调用点会发出的那个名字。

> **继承来的方法**：跨包元数据会把继承方法展平进每个派生类，所以「派生类上找得到」不等于
> 「派生类声明了它」。`methodof(Derived.InheritedMethod)` 会沿基链找到**真正的声明类**并校验
> 该函数确实被发射过，绝不发出一个没人能解析的名字。

## 指不了的方法（诚实的残余）

以下情形 **明确报错、不假装支持**：

| 情形 | 为什么 |
|---|---|
| 用户定义的**运算符与转换** | 它们**根本没有名字**；`op_*` 是编译器内部拼写，不是语言的一部分。改写成具名方法再指代 |
| **泛型基类替换后同签名** | `class C<T> { void Bar(int,T); void Bar(T,int); void Bar(int,int); }` 在 `D : C<int>` 上三个候选替换后全是 `(int,int)`，参数类型列表也分不开 |
| owner 带**类型实参**（`methodof(Box<int>.Get)`） | 泛型类**共享一份发射**，`Box<int>.Get` 与 `Box<string>.Get` 是同一个 qualified 名 ⇒ 区分没有意义。写裸名 `methodof(Box.Get)` |
| 方法带**类型实参**（`methodof(Seq.Map<int>)`） | v1 按名字指代泛型方法；类型实参形式的消歧未纳入 |

**逃生口是硬要求**：每条诊断都列出该名字下**全部可用重载**（本地候选还附声明位置），让用户
照着补签名即可。Swift 的 `@derivative(of:)` 没留这一步，用户撞上 ambiguous 只能干瞪眼。

## 诊断

| 码 | 场景 |
|---|---|
| `E0459` | 目标方法不存在 / 没有重载匹配给出的参数类型列表（列出全部可用重载） |
| `E0460` | 参数类型列表匹配到多个 / 未给列表但存在重载（列出候选 + 给出补签名写法） |
| `E0461` | 目标是运算符或转换——明确说明「指不了」而非含糊报找不到 |
| `E0462` | 语法形态错：缺 `Type.Member`、owner 或方法带类型实参 |

诊断的下划线落在**成员名本身**，不是整个 `methodof(...)` 表达式。

## 为什么不用 `&`

`&` **不用于**方法引用，留给将来的非托管函数指针：

| | 产出 | 性质 |
|---|---|---|
| `methodof(Logger.Log(string))` | `MethodInfo` | 托管、带元数据、**可经工厂函数序列化进 attribute** |
| `&Logger.Log`（将来） | `funcptr<…>` 之类 | 裸地址、无元数据、**不可序列化**、interop 用 |

先例：C++26 面对同一局面（既要 pointer-to-member 又要反射句柄）给了**两个不同的符号**
（`&Logger::log` 与 `^^Logger::log`）。而且 attribute 位置**结构性没有 target type**，用同一个
`&` 会造成无法消解的永久语法债。z42 的 native interop 需求真实存在（`[Native]` / extern），
函数指针大概率会来。

## 不在本特性范围内

- **IDE 的 Find All References / rename 跟随**：z42 目前没有 LSP 实现，`z42-lsp` 在
  roadmap 0.5.7。`methodof` 交付的是**编译期报错**（改名 / 删除 / 签名漂移 → 引用点变红），
  这一半是完整的；IDE 那一半依赖 LSP 里程碑。
- `fieldof` / `propertyof`：同家族，等 `methodof` 稳定后按各自收益立案。
