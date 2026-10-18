# generic_iface_param_cross_pkg — 泛型方法的接口实例化参数跨包可调

**这道门盯的是：TSIG 方法签名里的实例化接口成分必须写 FQ + 实参。**

`target` 包声明 `IVisitor<R, D>` 与 `Node.Accept<R, D>(IVisitor<R, D> v, D d)`——参数类型是
**本包接口的实例化、实参是方法自己的型参**；`main` 包实现接口并跨包调 `Accept`（显式 `<int, string>`
与省略推断各一次）。期望 `42 42`。

## 为什么这是真门

`TypeNameResolver._hybridTypeName`（类方法 + 自由函数的 TSIG 参数/返回拼写）此前把实例化接口落到
`_resolvedTypeName` 的末段 `t.Name()`——裸名、丢实参。消费包 `ImportedSymbolLoader._resolve` 按
FQN 表查这个参数类型 miss → `Builtin("IVisitor")` 垃圾伪类；普通（非泛型）调用的实参检查有垃圾
吸收、静默通过，而**泛型调用点的实参补查没有** ⇒ E0402「cannot assign … to (argument)」——
z42c.emission 调 `BoundExpr.Accept<R, D>(IBoundExprVisitor<R, D> v, D d)`（visitor 2/3）时实测踩中。
写成 `Fqn()<实参…>` 后，导入侧泛型实例化分支按 FQ 基名精确命中。
