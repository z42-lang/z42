# assoc_type_cross_pkg_mismatch（负例：跨包关联类型错绑定）

**assoc-type-crosspkg** 的跨包判别力回归门。

- `target`（`demo.assocmisstarget`）：`interface IEnum { type Item; }` + `class StrBag : IEnum { type Item = string; }`
- `main`（`demo.assocmissapp`）：`Holder<T> where T : IEnum<Item = int>` + `new Holder<StrBag>()`
- `expected_build_error.txt`：期望 main 编不过，输出含 `binds `Item` to `string`, but `int` is required`（E0453）。

## 为什么这是真门

本 change 之前，类的关联类型绑定 + 接口关联类型名单都无 wire 承载（导入侧恒空），
`ConstraintChecker._checkAssocBinding` 被迫用 `if (cls.IsImported) return;` 守卫跳过跨包 ⇒ 此负例
**静默通过**（漏报）。本 change 用 zbc 1.42 的 TYPE 统一 assoc 块承载类侧绑定、删守卫后，跨包错绑定
才被 E0453 抓住。正例（跨包正确绑定 Item=int 放行 + 运行 7/9/11）见 `assoc_type_cross_pkg`。
