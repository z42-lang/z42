# 静态成员的名字解析

> 对齐日期：2026-09-14 · change `add-static-properties`

静态字段、`const`、静态属性有两种写法，语义对标 C#：

```z42
class Counter {
    static int count = 0;
    const int Step = 2;
    public static int Total { get; set; }

    public Counter() { count += Step; }            // 裸名：≡ Counter.count / Counter.Step
    public int Peek() { return count; }            // 实例方法里同样可用
    public static void Reset() { count = 0; Total = 0; }
}

Counter.count = 5;       // 限定名（类外，或类内想写明时）
Counter.Total += 1;      // 限定名复合赋值
```

## 规则

在类的**任何成员体**内（实例 / 静态方法、实例 / 静态 ctor、属性 getter、索引器访问器、实例字段初始化器），
裸名 `x` 按下面顺序解析：

1. 局部变量 / 形参（含 lambda 形参）——**遮蔽优先**；
2. `this` 与本类（含继承来的）**实例**字段 / 属性；
3. **本类**的静态字段 / `const` / 静态属性 ⇒ 与 `C.x` 完全等价（访问控制、弃用、E0452 边界都一样）；
4. 枚举类型名、自由函数名（方法组）；
5. 都不是 ⇒ `E0401 undefined`。

lambda 里的裸名静态成员**不是捕获**：读到的是调用时的当前值。

## 不支持（保持 E0401）

- 派生类里用裸名引用**基类**的静态成员，以及 `Derived.baseStatic`——写 `Base.x`。
- 嵌套类里用裸名引用**外层类**的静态成员——写 `Outer.x`。
- 静态字段初始化器里用裸名引用同类其它静态成员（`static int b = a + 1;`）——写 `C.a`。

## 历史：为什么这曾经是静默错误

`add-static-properties` 之前，体绑定环境把 `ct.Fields` **全部**（含 static / const）定义成变量，发射端也把它们
放进「裸字段表」⇒ **实例方法**里的裸名静态字段 / const 被发成 `field_get this.x`，**静默读回 Null**；
**静态方法**里又什么都没定义 ⇒ E0401。现有用例全写成 `Counter.count`，所以长期没暴露。

## 实现

- 体绑定环境只 `Define` **非 static** 字段（`DeclBinder._defineInstanceFields`，原先 6 处手写循环收敛为一处）；
  `FunctionEmitter` 的裸字段表同样只收非 static。
- `ExprTyper._bindIdent` 在 `LookupVar` 落空后调 `MemberResolver.BindBareStatic`，它与限定名 `C.x` 走
  **同一个** `BindStaticMember` 判据，产出 `BoundStaticGet`——之后读、写、`++`、复合赋值全部复用 `C.x` 的既有路径。
- `C.x += v` 此前报 `E0401 undefined: C`：`AssignTyper` 为 event `+=`/`-=` 做的拦截先把接收者当表达式绑定，
  `=` 分支早有「接收者是类名就跳过」的判定、`+=` 分支漏了；两处现共用 `_isStaticRecv`。

静态属性的访问器派发见 [属性与索引器 · 静态属性](member-accessors.md#静态属性add-static-properties)。
