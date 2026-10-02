# iface_static_impl_mismatch（负例：跨包 static abstract 成员实现成 instance）

**fix-imported-iface-static-fidelity** 的跨包回归门。

- `target`（`demo.ifacestattarget`）：`public interface INum2 { static abstract int MakeZero(); }`
- `main`（`demo.ifacestatapp`）：`struct Bad : INum2 { public int MakeZero() {...} }`——把 static
  abstract 成员实现成 **instance** 方法。
- `expected_build_error.txt`：期望 main 编不过，输出含
  `is \`static\` in the interface and an instance method here`（E0412）。

## 为什么这是真门

本 change 之前，接口方法 static 位从 zbc wire 丢失（接口方法块零 flags 字节 → 恒 false），
`InheritanceResolver._checkOneIfaceMethod` 被迫用 `if (it.IsImported) return;` 跳过导入接口的
static 校验 ⇒ 此负例**静默通过**（main 编得过，漏报）。

本 change 给接口方法块加 `is_static:u8`（zbc 1.41），`TsigReconcile` 用真值构造、导入侧
`mz.IsStatic` 恢复真值、删守卫后此负例才真正被 E0412 抓住。**退回对照**：重加守卫或回退
is_static 承载 → main 恢复编得过 → 本 fixture（期望 build error）变红。

正例覆盖见 `src/tests/operators/static_abstract_operator.z42`（`struct Money : INumber`，
INumber 导入自 z42.core，5 个 `public static override` 正确实现 → 应放行）。
