# path 依赖闭包（两层）

exe `bar` → lib `foo` → lib `baz`。`z42c build bar --release` 要：自动建**整个闭包**；bar 解析到 foo 的符号；
**闭包全体**（foo + baz）colocate 进 bar 的 dist；bar.zpkg 直跑。

- **第二层（baz）才是重点**：深度为 1 时「直接依赖」恰好等于「闭包」，只搬直接依赖的缺陷会被完全遮住
  （编译期正常，运行期才 `MissingSymbolException`）。bar **故意不声明 baz**——它没引用过 baz 的符号。
- **baz 的清单故意用裸 `z42.toml`**（`z42 new` 写出的形态），另两层用 `<name>.z42.toml`：两种拼写都要能当 path 依赖。
- `--release`：私有 path 依赖走「colocate + 运行期惰性加载」，惰性加载只认 packed 依赖 zpkg。
