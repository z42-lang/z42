# struct_layout_crossns_collision — 同包跨命名空间同名 struct 各用各的布局

**这道门盯的是：struct 布局表与泛型类型声明表的键必须带命名空间，实例化名的类型实参也写全名。**

`target` 包里 `Demo.SlColl.A` 与 `Demo.SlColl.B` 各声明一份同名 `struct P`（字段不同）与泛型
`struct Pair<T>`（字段顺序不同）；`Use.z42`（第三个命名空间）把两边各建一份、读回字段，并经同一个
泛型函数 `Probe.Echo<T>` 各过一遍（值类型实参 ⇒ 特化体）。期望 `a7 b9 1|a7 2|b9 a7 b9`。

## 为什么这是真门

布局表若按短名建，`P` / `Pair$1` 只登记先到的那份：另一命名空间的 `P` 查不到布局 ⇒ 编译器崩在
`struct field path: layout lookup failed`；侥幸查到的则按**另一个类型的字段偏移**读写——静默读错值。
实例化名 `Pair$1<P>` / 特化名 `Echo:P` 的实参若只写短名，两个 `P` 落到同一个名字上，同样串。
