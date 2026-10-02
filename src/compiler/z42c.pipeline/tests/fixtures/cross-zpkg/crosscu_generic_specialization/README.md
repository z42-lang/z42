# crosscu_generic_specialization — 泛型体的单调化必须**跨编译单元**可见

**这道门盯的是：泛型声明在别的文件里时，调用点仍要改派到特化体。**

`target` 的 `Decls.z42` 声明 `ReadSecond<T>(Loc<T,int>)` 并在**同文件**用一次；
`Use.z42` 在**另一个文件**里用一次。两处都必须读出各自的 `Item2`（7 / 9）。

## 修前行为（实测，非推断）

`IrGen` 由 `CuCompile._compileCu` **按编译单元**创建。登记表若只扫本 CU 的 decls，
编 `Use.z42` 时看不见 `Decls.z42` 里的泛型声明 ⇒ 调用点判不出该改派 ⇒ 落到**擦除体**
⇒ 运行期 `struct ref leaf at byte offset 8 not in type layout`。

修法 = 登记表提到**包级**（`IrDump.BuildPackageCus` 扫全包 CU，与 `layouts` 同为并行段
只读共享），`Generate` 在未注入时才按本 CU 自扫（单文件 dump / 测试路径）。

## 顺带覆盖的第二件事

两个 CU 都用到 `ReadSecond<P2>` ⇒ **各发一份**同名特化体。本 fixture 把该包当依赖加载，
验证这不会被惰性加载器记成歧义函数（那会让调用当场抛）。
