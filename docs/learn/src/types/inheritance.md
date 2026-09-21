# 继承与多态

上一章每个类都是独立的。但真实的类型常常**一脉相承**：狗和猫都是动物，矩形和圆都是形状。
继承让你把共同的部分写一次，各自不同的部分分别写。

## 基类与子类

```z42
// examples/types/inheritance/basic/basic.z42
{{#include ../../../../examples/types/inheritance/basic/basic.z42:decl}}
```

`class Dog : Animal` 读作「Dog 是一种 Animal」。`Dog` 自动拥有 `Animal` 的成员（`Name`），
另外自己改写了 `Speak`。

三个关键词：

- **`virtual`**——基类说「这个方法子类可以改写」。**不写 `virtual` 就不能被改写。**
- **`override`**——子类说「我要改写它」。
- **`base(...)`**——子类构造器调基类构造器，把基类那部分先初始化好。

## 多态：按真实类型派发

```z42
// examples/types/inheritance/basic/basic.z42
{{#include ../../../../examples/types/inheritance/basic/basic.z42:use}}
```

```console
{{#include ../../../../examples/types/inheritance/basic/run.console:run}}
```

数组的元素类型是 `Animal`，但每个元素 `Speak()` 的结果**不一样**——调用哪个版本是由对象
**运行时的真实类型**决定的，不是由变量声明的类型。这就是多态。

它的价值在于：`foreach` 那段代码**不需要知道**有哪些动物。将来加一个 `Bird`，这段一个字
都不用改。

## 构造顺序：先基后派生

```z42
// examples/types/inheritance/ctor/ctor.z42
{{#include ../../../../examples/types/inheritance/ctor/ctor.z42:code}}
```

```console
{{#include ../../../../examples/types/inheritance/ctor/run.console:run}}
```

输出顺序说明了一切：**基类构造器先跑完，子类构造器才开始**。这样子类的构造器体里，基类
那部分已经是好的。

> `: base(...)` 与上一章的 `: this(...)`（转调同类另一个构造器）**只能二选一**。
> 不写 `: base(...)` 时会自动调基类的无参构造器——基类没有无参构造器就得自己写明。

## `abstract`：只定形状，不给实现

有时基类**根本没法给出实现**——「形状的面积」怎么算，得看是矩形还是圆。这时用 `abstract`：

```z42
// examples/types/inheritance/abstract/abstract.z42
{{#include ../../../../examples/types/inheritance/abstract/abstract.z42:decl}}
```

```z42
// examples/types/inheritance/abstract/abstract.z42
{{#include ../../../../examples/types/inheritance/abstract/abstract.z42:use}}
```

```console
{{#include ../../../../examples/types/inheritance/abstract/run.console:run}}
```

- `abstract` 方法**只有签名没有体**；含 `abstract` 成员的类自己也必须标 `abstract`。
- 子类**必须**实现所有 `abstract` 成员（否则自己也得标 `abstract`）。
- `Describe()` 是个普通方法，却能调还没有实现的 `Area()`——**基类定流程，子类填细节**，
  这是 `abstract` 最常见的用法。

### 抽象类不能 `new`

```z42
// examples/types/inheritance/abstract/noinstance.z42
{{#include ../../../../examples/types/inheritance/abstract/noinstance.z42}}
```

```console
{{#include ../../../../examples/types/inheritance/abstract/run.console:noinstance}}
```

道理很直白：`Shape` 没说面积怎么算，造出这样一个对象也没法用。

## `sealed`：到此为止

不想让别人再继承下去，就标 `sealed`：

```z42
// examples/types/inheritance/sealed/sealedcls.z42
{{#include ../../../../examples/types/inheritance/sealed/sealedcls.z42:code}}
```

```console
{{#include ../../../../examples/types/inheritance/sealed/run.console:run}}
```

再想继承它就会被拦下：

```z42
// examples/types/inheritance/sealed/nosubclass.z42
{{#include ../../../../examples/types/inheritance/sealed/nosubclass.z42}}
```

```console
{{#include ../../../../examples/types/inheritance/sealed/run.console:nosubclass}}
```

`sealed` 也能单独标在 `override` 方法上，意思是「这个方法改写到我为止」。

## 几条边界

- **单继承**：一个类至多一个基类。想要「多重」能力，用**接口**（下一章）。
- **`struct` 不参与继承**——值类型没有基类链。
- 改写时**形参类型必须完全一致**，否则不构成 `override`。

> 熟悉 C# 的读者请注意：z42 **没有 `new` 方法隐藏**（method hiding）。要么 `virtual` +
> `override` 真改写，要么就是两个不相干的方法——不存在「同名但不多态」的中间状态。

完整规则见参考手册的
[继承与多态](https://z42-lang.github.io/z42/reference/language/inheritance.html)。

## 小结

- `class B : A` 表示「B 是一种 A」；**单继承**，`struct` 不参与。
- `virtual` 允许改写，`override` 执行改写；**不写 `virtual` 就不能被改写**。
- **多态按运行时真实类型派发**——同一段循环能处理将来才加进来的子类。
- 构造顺序**先基后派生**；`: base(...)` 与 `: this(...)` 二选一。
- `abstract` 只给签名，子类必须实现；**抽象类不能 `new`**。
- `sealed` 封住继承链；z42 **没有 `new` 方法隐藏**。

下一章讲**接口**——不靠继承也能约定「能做什么」。
