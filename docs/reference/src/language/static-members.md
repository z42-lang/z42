# 静态成员的名字解析

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
3. 静态字段 / `const` / 静态属性 / 静态方法，依次找：**本类** → 本类的**基类链** → **外层类型**（嵌套类型体内，
   含外层的基类链，逐层往外）⇒ 与「声明类.x」完全等价（访问控制、弃用、E0452 边界都一样）；
4. 枚举类型名、自由函数名（方法组）；
5. 都不是 ⇒ `E0401 undefined`。

lambda 里的裸名静态成员**不是捕获**：读到的是调用时的当前值。

## 不支持（保持 E0401）

- 经**派生类名**限定访问基类的静态成员 `Derived.baseStatic`——写 `Base.x`（静态成员不是虚的，`Derived.x`
  读到的永远是基类那一份，写成派生类名容易误读成「派生类自己的」）。需要「每个类型各提供一份、按类型分派」的
  静态实现时，用接口的 `static abstract` 成员加型参约束（`where T : INumber<T>` 后写 `T.Zero`）。
- 静态字段初始化器里用裸名引用同类其它静态成员（`static int b = a + 1;`）——写 `C.a`。

## 实现

- 体绑定环境只 `Define` **非 static** 字段（`DeclBinder._defineInstanceFields`）；
  `FunctionEmitter` 的裸字段表同样只收非 static。
- `ExprTyper._bindIdent` 在 `LookupVar` 落空后调 `MemberResolver.BindBareStatic`，它与限定名 `C.x` 走
  **同一个** `BindStaticMember` 判据，产出 `BoundStaticGet`——之后读、写、`++`、复合赋值全部复用 `C.x` 的既有路径。
- `C.x += v` 的接收者是类名：`AssignTyper` 为 event `+=`/`-=` 做的拦截不能先把接收者当表达式绑定，
  所以 `=` 与 `+=` 两个分支共用「接收者是类名就跳过」的判定 `_isStaticRecv`。

静态属性的访问器规则见[属性与索引器](properties-indexers.md)。
