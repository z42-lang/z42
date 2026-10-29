# iface_overload_dispatch_cross_pkg

接口调用按**接口槽**派发（fix-interface-overload-dispatch）的跨包守卫。

接口、实现类、调用点分处两个包：接口槽表由**实现类所在包**的编译器按签名定案、随 TYPE 段下发，
调用点（可在任一包）发接口槽键，运行期经 vtable 别名落到实现。

覆盖：
- 实现类有同名重载、接口要的那个不是声明序首个（`W`）；
- 同一泛型接口实现两次（`X : IFoo<int>, IFoo<string>`）；
- 跨包基类 + 本包派生类新增同名重载（`D0 : B0`）—— 不得顶掉基类的裸键槽；
- 跨包类的 override 经接口调用生效（`W2 : W`）；
- 本包类实现跨包接口（`Mine`）；
- 调用点在依赖包里、实现在本包（`Calls.ViaPlain` / 共享泛型体 `Calls.ViaFoo<T>`）。
