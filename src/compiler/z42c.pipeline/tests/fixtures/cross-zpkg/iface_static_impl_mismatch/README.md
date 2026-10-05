# iface_static_impl_mismatch（负例：跨包 static abstract 成员实现成 instance）

跨包接口 static 位保真的回归门。

- `target`（`demo.ifacestattarget`）：`public interface INum2 { static abstract int MakeZero(); }`
- `main`（`demo.ifacestatapp`）：`struct Bad : INum2 { public int MakeZero() {...} }`——把 static
  abstract 成员实现成 **instance** 方法。
- `expected_build_error.txt`：期望 main 编不过，输出含
  `is \`static\` in the interface and an instance method here`（E0412）。

## 为什么这是真门

接口方法块携带 `is_static:u8`（zbc），`TsigReconcile` 用真值构造、导入侧 `mz.IsStatic` 恢复真值，
`InheritanceResolver._checkOneIfaceMethod` 才能对导入接口做 static 校验并报 E0412。
若 static 位在 wire 上丢失（恒 false），该校验对导入接口无从判断，此负例会静默通过（main 编得过，漏报）。

正例覆盖见 `src/tests/operators/static_abstract_operator.z42`（`struct Money : INumber`，
INumber 导入自 z42.core，5 个 `public static override` 正确实现 → 应放行）。
