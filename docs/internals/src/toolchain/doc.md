# 文档索引与 `z42 doc`

> 代码：`src/compiler/z42.doc/src/`（引擎，含 `DocParser.z42`）、`src/toolchain/devtools/core/doc_cmd.z42`（`DocCommand`）、
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
                 ├─ SyntaxIndexer   Parser → AST → DocItem（doc 原文来自 Decl.Doc）
                 ├─ DocParser       doc 原文 → DocComment（摘要 / 正文 / tag / 链接 / 示例）
                 ├─ DocIndex        名字解析 / 成员 / 搜索
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
  收进 Examples 并去公共缩进，`@hidden` 置 Hidden 位，``[`Sym`]`` 抽进 Links。校验（名字是否存在、类型与
  链接能否解析）留给语义层。

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

## 5. 边界与限制

- 只索引 z42 源码；Rust 运行时的符号索引还没有。
- 只有语法级：没有继承文档、实现者列表；`@tag` 已解析，但链接校验、`@param` 名校验还没有。
- 还没有书页反查（`--page` / `--docs`）与书里代码引用的门禁。

以上后续阶段见仓库根 `docs/z42-doc.md`。
