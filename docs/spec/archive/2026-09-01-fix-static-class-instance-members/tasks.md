# tasks · fix-static-class-instance-members

- 🟢 1. 调查：全代码库扫描 `static class` + 实例成员 → 唯一 offender = `Std.String`
- 🟢 2. stdlib：`String` `static class` → `sealed class`（对齐 C# System.String）
- 🟢 3. 编译器：`DiagnosticCodes.z42` 登记 `StaticClassInstanceMember = "E0451"`
- 🟢 4. 编译器：`InheritanceResolver._passSealedEnforce` 加 static-class 强制（基类 / 接口 / 实例成员）+ `_checkStaticClassMember` helper（字面量 "E0451" 发码）
- 🟢 5. 测试：collect_tests 加 7 个 E0451 用例（实例方法/字段/构造器/基类/接口 报错 + 静态成员/非static类 不报）
- 🟢 6. GREEN：`xtask build compiler` + `build stdlib` + `test compiler`（23 [Test] 单元 + 不动点 3/3）+ `test stdlib`
- 🟢 7. 文档：`docs/book/src/language/static-classes.md` 新页 + SUMMARY 挂载
- 🟢 8. bootstrap 边界检查：`xtask test bootstrap` 无越界（上一 nightly 编当前源 OK）
- 🟢 9. 归档 + PR（合并前并入 main 最新 + 重跑 GREEN）
