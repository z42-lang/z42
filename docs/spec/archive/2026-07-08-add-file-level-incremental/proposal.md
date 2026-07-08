# Proposal: 文件级增量编译——cache 为编译 SoT，dist 为 cache 的确定性投影

## Why

1. **当前增量是整包粒度**（port-incremental-build-cache，2026-07-05）：任一源文件变化 →
   整包全量重编。对多文件包（launcher 5 文件、未来用户工程更多），改一个文件重编全部
   是不必要的浪费。
2. **User 目标架构（2026-07-07 裁定）**：cache 里是**单文件编译产物 + 源文件信息**，作为
   增量判定与组装的 SoT；**dist 基于 cache 输出**——cache 哪个文件变了就只重编哪个（及其
   包内依赖方），packed 模式任一 cache 变更即重写整个 zpkg；产物确定性不变（源不变 →
   字节不变）。这也是后继 indexed 最小 patch 分发（`add-indexed-zpkg-min-patch`，DRAFT）
   的直接前置——indexed 的散装 zbc 就是 cache 的 fullMode zbc 形态。

## What Changes

1. **cache 条目升级为 SoT**：`<cache>/<rel>.zbc`（fullMode，已有）旁增 `<rel>.meta`
   （cache 内部格式，非 wire 格式）：源 SHA-256、ns、usedDepNs、包内引用的定义文件集、
   writer 版本 pin（zbc/zpkg minor——版本不符整体失效）。增量判定不再读上次 zpkg 的 MODS。
   （**D2 修正 2026-07-08**：TSIG 不入 meta——每文件 TSIG 全包耦合（自由函数兄弟泄漏），
   每次 build 由当前 AST 全包重算，见 design D2。）
2. **文件级依赖图 + 传递失效**：probe 求失效集 = {源 hash 变化的文件} ∪ 其**包内传递
   依赖方**（A 引用 B 定义的符号 → B 变则 A 失效）。边来自 meta 记录的「引用定义文件集」
   （符号→定义文件归属由各文件 TSIG 提供）。**保守边**：被依赖文件任何变化即失效依赖方；
   「仅 TSIG 签名变化才失效」的细化留 Deferred（先保正确性）。
3. **失效子集重编，未失效文件从 cache 重建**（D2 修正 2026-07-08）：全包照常 parse +
   符号收集 + TSIG 重算（轻量前端）；cached 文件仅跳过 typecheck+codegen，其 IrModule
   由 ZbcReader 从 cache zbc 读回。与 C# 被放弃的混合重建（`ed901f01` 前）的本质区别：
   **无跨代元数据合并**——TSIG/符号一律出自当前源 AST（终极 SoT），zbc 出自 hash 校验
   一致的 cache，不存在「旧 zpkg 元数据 + 新编译产物」两来源合并的不一致。
4. **dist = cache 的纯函数**：`BuildPackedD` 输入一律源自 cache 条目集；任一 cache
   条目变更 → 重写整个 packed zpkg；全部未变 → 完全跳过（沿用现状）。
5. **硬验收 = 暴力对账器**：新增 xtask 自检——对语料（z42c 7 包 + 代表性 stdlib 包 +
   launcher）**逐文件**模拟 touch，断言「增量产物 == `--no-incremental` 全量产物」逐字节
   相等；再叠加既有自举不动点 7/7 门禁。任一不等 = 未完成。
6. **workspace/flat 路径仍不走增量**（沿用 port-incremental-build-cache 边界；
   `incremental-future-workspace-wiring` 另行处理）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.project/src/CacheStore.z42` | NEW | cache 条目读写：zbc + meta（序列化 TSIG / 源信息 / 依赖集 / 版本 pin）|
| `src/compiler/z42c.pipeline/src/IncrementalBuild.z42` | MODIFY | 判定 SoT 切到 cache；依赖图 + 传递失效；输出失效集而非 all-or-nothing |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `_build`：失效子集重编 + cached 条目重建注入 + dist 从 cache 组装 |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | `BuildPackage` 支持注入外部（cached）Exported 到共享收集器（挂点见 design D2 / 调查任务 1.1）|
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcReader.z42` | NEW | fullMode zbc → IrModule 读取器（cached 文件重建用；本 change 起有真实消费方）|
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | 一行访问器 `Tokens()` 暴露内部 Lexer（D8 性能修正：parse 时捕获标识符，消第二遍 lex；User 追认 2026-07-08）|
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | `ReadSourceHashes` 退役或降级为迁移期兜底（判定 SoT 移至 cache）|
| `src/compiler/z42c.pipeline/tests/incremental/` | MODIFY | 单测扩展：依赖失效判定 / meta 往返 / 版本 pin 失效 |
| `src/compiler/z42c.ir/tests/zbcreader/` | NEW | ZbcReader 往返单测（Write→Read→Write 字节相等）|
| `scripts/test/xtask_test_incremental.z42` | NEW | 暴力对账器（逐文件 touch 增量 vs 全量逐字节比对）+ 注册进 test 命令面 |
| `scripts/xtask_cli.z42` | MODIFY | 注册暴力对账器子命令 |
| `src/compiler/z42c.project/README.md` | MODIFY | CacheStore / 消费面 |
| `src/compiler/z42c.pipeline/README.md` | MODIFY | IncrementalBuild 语义更新 |
| `src/compiler/z42c.driver/README.md` | MODIFY | 增量节更新 |
| `src/compiler/z42c.ir/README.md` | MODIFY | ZbcReader |
| `docs/design/compiler/project.md` | MODIFY | 增量编译节：文件级形态 + cache SoT 模型 |
| `docs/book/src/dev/build.md` | MODIFY | 增量小节同步 |
| `docs/roadmap.md` | MODIFY | Deferred 索引（签名级失效细化）|
| `docs/spec/changes/ACTIVE.md` | MODIFY | compiler 锁登记/释放 |

**只读引用**：`src/compiler/z42c.project/src/{ZpkgBuilder,ZpkgWriter,PackageTypes}.z42`、
`src/compiler/z42c.semantics/src/{SymbolCollector,ExportedTypeExtractor}.z42`（注入面调查）、
`docs/design/language/generics.md`（代码共享确认，非单态化拷贝——文件级失效的格式前提）。

## Out of Scope

- indexed zpkg / 最小 patch 分发 —— 后继 change `add-indexed-zpkg-min-patch`（DRAFT 已立）。
- workspace/flat 构建增量布线 —— `incremental-future-workspace-wiring`。
- zbc/zpkg wire 格式变更：**零 bump**（meta 是 cache 内部文件，带自身版本 pin，不是 wire 格式）。
- 跨包（上游 zpkg 变化 → 下游失效）追踪 —— 维持现状（上游变化不触发下游源码 hash 变化，
  由构建编排层保证顺序重建；正式追踪留 indexed change 一并评估）。

## Open Questions

- [ ] cachedExports 注入 PS-1 共享收集器的具体挂点（任务 1.1 调查；C# 对应
  `TryCompileSourceFiles(freshFiles, cachedExports)`）。
- [ ] 同包跨文件 token/字符串池是否存在「B 变化但 A 源不变时 A 的 zbc 字节漂移」的隐藏耦合
  ——暴力对账器（What 5）就是证伪工具；若存在则失效边按实测收紧。
