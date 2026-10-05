# assoc_type_cross_pkg_mismatch（负例：跨包关联类型错绑定）

跨包关联类型的判别力回归门。

- `target`（`demo.assocmisstarget`）：`interface IEnum { type Item; }` + `class StrBag : IEnum { type Item = string; }`
- `main`（`demo.assocmissapp`）：`Holder<T> where T : IEnum<Item = int>` + `new Holder<StrBag>()`
- `expected_build_error.txt`：期望 main 编不过，输出含 `binds `Item` to `string`, but `int` is required`（E0453）。

## 为什么这是真门

类的关联类型绑定与接口关联类型名单经 zbc TYPE 的统一 assoc 块承载，导入侧才能还原；若承载断开，
`ConstraintChecker._checkAssocBinding` 对导入类无从校验，此负例会静默通过（漏报）。
正例（跨包正确绑定 Item=int 放行 + 运行 7/9/11）见 `assoc_type_cross_pkg`。
