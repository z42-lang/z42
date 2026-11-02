# 文档索引与 `z42 doc`

> 代码：`src/compiler/z42.doc/src/`（引擎，含 `DocParser.z42` / `DocLint.z42` / `DocRefs.z42` / `DocMd.z42`）、`src/toolchain/devtools/core/doc_cmd.z42`（`DocCommand`）、
> `src/toolchain/devtools/rust-index/`（Rust 索引器）、`scripts/build/xtask_doc.z42`（`doc gen` / `doc index` CLI）、
> `scripts/common/xtask_doc_index.z42`（Rust 索引构建）、`scripts/check/xtask_check_doc_lints.z42` / `xtask_check_doc_refs.z42`（检查与报告）、
> `src/toolchain/launcher/core/launcher_cli.z42`（`_forwardZ42d`）、`src/compiler/z42c.syntax/src/DocBlock.z42`（doc 挂靠）
>
> 要查**怎么用**（旗标、输出、名字怎么写）→ 语言与库参考的 `z42` 命令页与 doc 注释页。

本页写 `z42 doc` 是**怎么实现的**：一次查询从命令行到输出经过哪几层、索引里的符号身份（Doc ID）怎么定、
名字按什么顺序解析、缓存凭什么判断失效。改查询行为、输出格式或索引范围时读它。

## 1. 分层

```
z42 doc <args>                      launcher：`doc` 原样转发
  └─ z42vm programs/z42d/z42.devtools.zpkg -- doc <args>
       └─ DocCommand                z42d：命令行 → DocQueryOptions；定索引根与缓存路径；打印
            └─ DocQuery.Run         z42.doc：建索引 → 解析名字 → 选输出形态（纯函数，有单测）
                 ├─ SourceTree      找根、遍历 .z42、按最近的清单归包
                 ├─ IndexCache      按文件增量：未变的文件抄缓存，变了的交给 SyntaxIndexer
                 ├─ ExternalIndex   并入 <根>/artifacts/doc/*.idx（Rust 运行时的条目）
                 ├─ SyntaxIndexer   Parser → AST → DocItem（doc 原文来自 Decl.Doc）
                 ├─ DocParser       doc 原文 → DocComment（摘要 / 正文 / tag / 链接 / 示例）
                 ├─ DocIndex        名字解析 / 成员 / 搜索
                 ├─ DocMd           --format mdbook：包索引 → reference 包页 markdown
                 └─ DocFormat       文本或 JSON（正文与 tag 经 DocParser 结构化渲染）
```

逻辑都在编译器域的 `z42.doc` 包里，z42d 只做薄胶水：工具链目录没有测试基础设施，而编译器包的扁平
`tests/*.z42` 由 `xtask test stdlib` 自动发现。`z42.doc` 不是编译器 driver 的依赖，SDK 的 `programs/z42c`
里没有它，所以 z42d 以 path 依赖引用、由 z42c 代建并连同 `z42c.syntax` / `z42c.core` 复制进 `programs/z42d`。

## 2. 语法级索引

只解析不做语义分析，所以快、能收私有成员、改完代码立即可查；代价是没有类型解析：

- **Doc ID** 的参数类型按源码写法记（`Ns.Type.Method(int,List<T>)`），不做全限定；同名不同命名空间的类型
  在参数表里分不开。语义级索引落地后由它给出全限定的 ID。
- 嵌套类型记作 `Outer+Inner`（与编译器展平嵌套类型的命名一致）；构造器记作 `Type.ctor(...)`，索引器
  `Type.this[...]`；`impl I for T` 的方法并入 `T`，目标写短名时按本文件命名空间补全。
- **可见性**：显式修饰符优先；否则接口成员 public、命名空间级 internal、类成员 private；枚举成员随枚举。
- 签名只用 AST 渲染；参数默认值只写出字面量、负数字面量、名字与成员链，其余记 `…`。
- 摘要 = doc 首段，遇空行或首个 `@` 行即止。
- `DocParser.Parse` 把 doc 原文拆成 `DocComment{Summary, Body, Tags[], Links[], Examples[], Hidden}`：
  只做文本解析，正文 Markdown 原样保留；`@param` / `@typeparam` / `@throws` 拆出第一段作名字，`@example`
  收进 Examples 并去公共缩进，`@hidden` 置 Hidden 位，``[`Sym`]`` 抽进 Links。校验分两层：不需要语义的
  （`@param` 名是否在签名里、tag 是否在闭集、摘要、历史注记、孤立块、重复 `//!`）由 `DocLint` 在语法索引上做
  （§8）；类型与链接能否解析留给语义层。

## 3. 名字解析

`DocIndex.Find` 按级别找，命中即停：

1. Id 完全相等；
2. 限定名（去掉参数表）等于查询，或以 `.查询` / `+查询` 结尾——必须在段边界上，`andom` 不命中 `Random`；
   查询自带参数表时按完整 Id 比后缀；
3. 同 2，忽略大小写；
4. 短名包含查询（忽略大小写）。

同名重载一起返回；`DocQuery` 再决定展开（单个命中或全是同一个名字）还是列候选。命名空间没写 `//!` 时没有
命名空间条目，按「有类型或自由函数挂在它下面」认，表头是合成的、不带位置。直接查名字时私有成员可见；
轮廓与搜索默认隐去 private，`--private` 打开。

## 4. 索引根与缓存

- **根**：起点向上第一个含 `.git` 的目录（目录或 worktree 的 `.git` 文件），否则最近的工程清单所在目录，
  都没有就用起点。遍历跳过隐藏目录与 `artifacts` `target` `node_modules` `tests` `benches` `examples` `fixtures`。
- **归包**：每个文件归到离它最近的上层清单（`z42.toml` 或 `<name>.z42.toml`，只认文件——stdlib 的 TOML 包
  目录就叫 `z42.toml`）。`--pkg` 只索引该包，且不读写共享缓存，免得把整份缓存冲掉。
- **缓存**：用户缓存根（与 `z42 run` 单文件工程、REPL 的命名空间索引同一规则，见 `UserCache.Root`）下
  `doc/<根路径的哈希>.idx`。行格式：表头记根路径，每个文件一条 `F`（相对路径、大小、修改时间、包名），
  其后是它的条目。查询时逐个 stat，大小和修改时间都没变就原样复用；有增删改就整份重写。表头不符（格式
  版本或根路径不同）整份作废。写缓存失败只损失速度。
- 实测：本仓库全量冷建约 8 秒，热查询约 0.4 秒，缓存约 2 MB。

## 5. mdbook 渲染（`z42 doc --format mdbook`）

`DocMd.PackagePage` 把一个包的语法级索引渲染成 `reference/stdlib/<pkg>.md`：页首「由 z42 doc 生成」
注释、标题（包名 —— `//!` 摘要）、包路径与命名空间行、`//!` 正文与 `@warning`、每个类型一个 `##`
（类型文档 + ```` ```z42 ```` 签名总览块）、每个 public 成员一个 `###`（正文、`@param` 表、`@throws`、
`@example`）。private 成员不进参考页。

- **成员导航**：每个成员带一个 `<a id>` 锚点（Doc ID 的 slug），类型标题下一行成员索引链到各锚点。mdBook 的
  SUMMARY 不支持锚点条目，成员级导航只能放在页内。
- **源码位置**：每个声明一行 `file:line`。`DocMd.SourceBase` 非空时渲染成链接，点击直达仓库源码的那一行；
  基址取 book.toml 的 `git-repository-url`（`blob/main`），`Z42_DOC_SOURCE_BASE` 可覆盖（本地预览 / 镜像）。
  生成页里的行号随每次 gen 重写，不参与 doc-refs 的手写页报告（§8）。
- **同名声明合并**：partial 类型并成一页（代表声明），泛型委托按元数各一页。
- **type-split**：太大的包一个类型一页——索引页 `<short>.md` 加每个类型的 `<short>.<Type>.md`（当前只有
  `z42.core`，清单硬编码在 xtask 里，待 `[doc] split` 清单键就绪后改读清单）。SUMMARY.md 里该包的页清单
  区段由 `xtask doc gen` 重写：标记注释夹住，按命名空间分草稿章、章下列类型；区段外不许生成改动。区段随
  SUMMARY 入库、不设门禁——发布前总会重新生成，入库那份落后只表现为本地 gen 后 `git status` 多一处改动。

`z42d doc --format mdbook --out <dir> --pkg <name>` 渲染一个包到 `<dir>/<name>.md`（生成不写共享索引缓存）。
`xtask doc gen` 生成全部写了 `//!` 的包的页面到 `docs/reference/src/stdlib/`，并删掉不再生成的旧页（该目录除
`README.md` 外全归 gen 所有）；内容不变的页不重写。**生成页不入库**（`.gitignore`），由用到它们的地方现生成：

| 谁 | 何时 |
|---|---|
| `deploy-book.yml` | 构建 reference 之前（xtask-only bootstrap 后 `xtask doc gen`）；stdlib 源码、`z42.doc`、`xtask_doc.z42` 的改动都触发它 |
| `xtask test docs`（GREEN gate 的 docs stage、CI docs-check） | 查链接之前——手写页链进生成页的链接、生成页自己的链接一起查 |
| 本地 | 读 / 预览 reference 的 stdlib 页之前手动跑一次 |

所以没有「生成页 ↔ 源码」的漂移门禁：发布物永远从当前源码渲染。代价是 GitHub 源码树里看不到这些页，看网站或
本地 gen。gen 用的解析器是 xtask 自带的 z42c.syntax，CI 上是种子 SDK 的那份——stdlib 用上种子还不认的语法时，
到下一次 nightly 前 deploy-book 与 docs-check 会红。

## 6. Rust 条目（外部索引）

仓库里的 Rust 运行时不归 `z42.doc` 解析，而是由 `xtask doc index` 先产出一份外部索引，`z42 doc` 查询时并入：

```
xtask doc index
  └─ cargo build src/toolchain/devtools/rust-index      （与 apphost 同一 cargo 配置）
  └─ z42-rust-index --root . --out artifacts/doc/rust.idx --regen "xtask doc index" z42=src/runtime/src/lib.rs …
       从每个 crate 根文件沿 `mod` 声明走模块树（`#[path]` 照认，`#[cfg(test)]` 跳过）→ syn 解析 → 条目行
```

- **格式**：表头 `z42doc-items 1` 加可选的重建命令，之后是 `F`（文件、crate）与 `I`（条目）行，条目行与本地
  缓存同一格式，读取复用 `IndexCache.ReadItem`。条目 id 是 Rust 路径（`z42::gc::ambient::HeapGuard::enter`）；
  名字解析在段边界上同样认 `::`，查询里写 `.` 也当 `::`。
- **种类**：`module` / `struct` / `enum` / `enum-member` / `trait` / `func` / `method` / `field` / `const` / `static`
  / `type` / `macro`。`module`、`trait` 与类型一样能列轮廓；查模块名得到模块本身的轮廓。
- **签名**按源码原样截取（可见性起到签名末尾，空白折叠）；doc 取 `///` / `//!` 编译成的 `#[doc]`。
- **可见性**：`pub` → public，`pub(crate)` 等 → internal，不写 → private；trait 里的方法与 trait impl 的方法记 public。
- **`impl` 方法**挂在 impl 所在模块里的同名类型下；跨模块的 impl 会挂到一个不存在的父条目，方法本身仍能按名字查到。
- **过期**：外部索引不会自己更新。加载时逐个比对源文件与索引文件的修改时间，有源文件更新或已删除就在 stderr
  提示重跑表头记的命令，查询照常进行。

## 7. 书里代码引用（`--page` / `--docs`）

`DocRefs` 解析书页（markdown）里的代码引用：页头「代码」行（`> 代码：`…` 及其后连续 `>` 行）里带反引号的
路径是**范围**；正文单反引号里的 `A.B` / `a::b` 符号链当作引用（末段是文件扩展名、或写成 `文件:行号` 的
分别按文件 / 行号处理——行号一律报 ✗）。`DocIndex.Resolve` 先精确后后缀解析，命中优先取文件落在范围内的。
书页里写的**包名**（如 `` `z42.core` ``）与命名空间同形，按索引项自带的包字段兜底解析，不算未解析。

- `z42 doc --page <md>`：读一页，逐行输出 `符号 → file:line`；解析不了的多段链标 `?`，行号引用标 `✗`。
- `z42 doc --docs <sym>`：反查，扫 `docs/**/*.md` 与 `src/**/README.md`，列出引用该符号的页与行。
- 代码块（``` 围起来的）里的内容不算。

同一套解析用于 `xtask check doc-refs` 的引用报告（§8）；改完一页可用 `--page` 查看 `?` / `✗`。

## 8. 注释检查与引用报告（`xtask check`）

doc-lints 是 GREEN 的一个 stage；doc-refs 是手动引用报告，不参与 CI 通过判定。

| 命令 | 行为 |
|---|---|
| `check doc-lints` | stdlib 各包的 doc 注释过 `DocLint` 语法级检查，任何 W08xx 发现即失败并列出位置 |
| `check doc-refs` | 报告书页与 README 中的行号引用、未解析多段符号链及各区域统计，不维护数量上限 |

**DocLint**（`z42.doc` 包的 `DocLint.CheckPaths`）在语法索引上做不需要语义的检查，按包分组跑（W0808
「同一命名空间一份 `//!`」按包判）：W0801 孤立块、W0802 `@param` / `@typeparam` 名不在签名里、W0804 未知
tag、W0805 public 声明缺 `///`、W0806 历史注记（日期 / spec 名模式）、W0807 摘要缺失或超长、W0808 重复 `//!`。
范围只有 stdlib：编译器域的注释按 doc-system「契约不显然时写」，不要求 W0805 全覆盖。W0803（链接与
`@throws` 类型能否解析）需要语义信息，归编译器的 DocChecker pass，不在此列。

**doc-refs** 跑之前先执行 `xtask doc index` 重建 Rust 外部索引（§6），Rust 符号按当前源码解析；生成页（页首
「由 z42 doc 生成」标记）自带随 gen 重写的 `file:line`，不查；learn 的引用对象在 `examples/`、不进符号索引，
归 learn↔examples 重放检查，也不查。

## 9. 边界与限制

- 只有语法级：没有继承文档、实现者列表。`@tag` 的语法级检查（§8 的 DocLint）已有；链接与 `@throws` 类型解析
  （W0803）需要语义，归编译器 DocChecker pass，尚未落地。
- `z42 doc` 查询用的 Rust 索引不会自己更新：手动 `xtask doc index`（过期会提示）；`check doc-refs` 每次自动重建。
- doctest（`@example` 合成测试、`z42 test --doc`）尚未接。

以上后续阶段见仓库根 `docs/z42-doc.md`。
