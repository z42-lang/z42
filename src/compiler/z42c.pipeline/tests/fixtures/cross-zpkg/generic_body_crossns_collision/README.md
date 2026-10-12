# generic_body_crossns_collision — 同包跨命名空间同名的泛型体各特化各的

**这道门盯的是：泛型体登记表的键必须带命名空间。**

`target` 包里 `Demo.GbColl.A` 与 `Demo.GbColl.B` 各声明一份同名的泛型静态方法 `Util.Tag<T>`、
实例泛型方法 `Box.Tag<T>`、泛型自由函数 `FreeTag<T>`，返回值各不相同；`Use.z42`（第三个命名空间）
把六个都按 `<int>` 调一遍。期望 `1 2 10 20 100 200`。

## 为什么这是真门

值类型实参的泛型调用会改派到**特化体**（`CallEmitter._specializeGenericBody` / `_specializeVCallName`），
特化体从包级登记表 `IrGen.GenericBodies` 取 AST。表若按短键（`Util.Tag` / `FreeTag`）建，先登记的那份
赢，另一个命名空间的调用会**静默特化出前一份的体**——编译零诊断、运行期返回错的值（`1 1 10 10 …`）。
