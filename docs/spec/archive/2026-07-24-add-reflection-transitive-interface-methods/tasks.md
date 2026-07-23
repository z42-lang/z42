# Tasks: 接口继承方法进 GetMethods（transitive interface methods）

> 状态：🟢 已完成 | 创建：2026-07-24 | 完成：2026-07-24 | 分支：feat/reflection-transitive-iface-methods（隔离 worktree z42-reflinst，User 授权）
> 类型：feat（小；纯运行期，闭 add-interface-member-reflection 的 transitive 剩余项）

**变更说明：** `interface IBar : IFoo` 时 `typeof(IBar).GetMethods()` 此前只返 IBar 直接声明的方法；
现并入其（传递）基接口的方法——镜像 C#（接口 GetMethods 返回整个接口集的成员）。

**原因：** add-interface-member-reflection（2026-07-20）只 surface 直接声明方法，transitive 延后。
GetInterfaces 早已做接口传递闭包（add-reflection-transitive-interfaces），GetMethods 未同步。

**修复（纯 runtime，复用现有闭包遍历）：** `builtin_type_methods` 在直接 `iface_methods()` 循环后，
**若 td 是接口**（`class_flags & CLASS_FLAG_INTERFACE`），BFS 展开 `td.interfaces()` 基接口闭包
（同 `builtin_type_interfaces` 遍历），并入各基接口 `iface_methods` 的 MethodInfo（按声明接口限定名
dedup）。仅对接口生效——类的具体实现已在 vtable，不重复灌抽象签名。**无 compiler 改动、无格式 bump。**

**文档影响：** `docs/design/language/reflection.md`（interface-member-reflection「剩余」标记 transitive 落地）。

- [x] 1.1 `reflection.rs` `builtin_type_methods`：接口 gated BFS 基接口闭包 + 并入 iface_methods
- [x] 1.2 `src/tests/types/transitive_interface_methods.z42`：e2e（一层/两层 transitive / 基接口不受影响 / 类不污染）——interp+jit 空输出 exit0
- [x] 1.3 全绿：types e2e **74 pass 0 fail**（get_interfaces / transitive_interfaces / interface_class_predicates 无回归）+ stdlib z42.core **44 pass 0 fail**
- [x] 1.4 `docs/design/language/reflection.md` 标记 transitive 落地
- [x] 1.6 `src/libraries/z42.core/tests/reflection.z42`：既有 `test_interface_getmethods_declared_only`（断言旧「仅直接声明」行为）更新为 `..._includes_transitive`（新 C# 对齐行为）——本变更**有意改变**该行为
- [x] 1.5 归档 + PR

## 备注
- z42c/stdlib 零改动 → 自举不动点 trivially byte-identical（不跑亦可，本 change 仅 reflection.rs）。
- dedup 按「声明接口.方法名」——同名方法来自不同接口各占一条（不同 DeclaringType），与 C# 一致。
