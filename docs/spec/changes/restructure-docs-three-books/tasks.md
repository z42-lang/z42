# Tasks: 文档三书重构

> 状态：🟡 进行中 | 创建：2026-09-16
> 分 8 批，每批一个 PR。**User 裁决：穿插在功能开发之间推进**，不连续做完。
> 逐文件搬迁清单见 [migration-manifest.md](migration-manifest.md)；角色与规范见 [design.md](design.md)。

## 进度概览
- [x] **批 0 · 立宪**（PR 待合）
- [x] 批 1 · internals/compiler（PR 待合）
- [x] 批 2 · internals/runtime + formats（PR 待合）
- [x] 批 3 · reference/language（3a PR #697 / 3b PR #703）
- [x] 批 4 · reference/stdlib
- [x] 批 5 · toolchain + devinfra
- [x] 批 6 · 收尾（philosophy / features / 矩阵拷贝 / grep 清零）
- [ ] 批 7 · 其余门禁

---

## 批 0 · 立宪（不搬任何内容）

### 0.1 总纲
- [x] 0.1.1 重写 `docs/agent/rules/doc-system.md`：三书角色表（含「明确不写什么」）、边界裁决表、
      三问触发判据、三道门、**单向链接铁律**（reference 不得链 internals）、**过渡期铁律**
      （design/workflow 冻结只读，要改先搬）
- [x] 0.1.2 `docs/agent/rules/book-writing.md`：三页型 → 三本书的映射（参考页→reference、机制页→internals、
      概览页→各书部分入口）；页头字段不变
- [x] 0.1.3 `../../../agent/rules/README.md` 与 `.claude/CLAUDE.md` 中指向 `docs/book/` / `docs/design/` 的
      描述改指三书（**注意 CLAUDE.md 末尾两行 `@import` 的是 design 文件，必须改**）

### 0.2 两本书的骨架
- [x] 0.2.1 `docs/reference/book.toml` + `src/README.md`（前言：这本书是什么、怎么查）
- [x] 0.2.2 `docs/reference/src/SUMMARY.md` 骨架 + 各部分 README：
      `language/` `stdlib/` `toolchain/` `embedding/` `appendix/`
- [x] 0.2.3 `docs/internals/book.toml` + `src/README.md`（前言 + 系统总览：三个进程怎么协作）
- [x] 0.2.4 `docs/internals/src/SUMMARY.md` 骨架 + 各部分 README：
      `compiler/` `runtime/` `formats/` `stdlib/` `toolchain/` `testing/` `devinfra/`
- [x] 0.2.5 两书 `theme/` 复用 learn 的 `z42-highlight.js`

### 0.3 发布与门禁
- [x] 0.3.1 `.github/workflows/deploy-book.yml` 扩到三书：`_site/learn` + `_site/reference` + `_site/internals`
- [x] 0.3.2 ~~站点根三书分流索引~~ → **挪到批 6**：`docs/book` 搬空前仍占着站点根
      （`mdbook build -d $site`），此刻放索引会与它冲突。批 6 删 book 时同批落地。
- [x] 0.3.3 **死链检查**（裁决 5）：`scripts/test/xtask_test_docs.z42` 新增，只做一件事——
      扫 `docs/**/*.md` 的相对链接，不可解析即红；接进 `xtask test` 的 stage 列表
- [x] 0.3.4 `ci.yml` 的 `paths-ignore` 放出 `docs/**`，让 0.3.3 在纯文档 PR 上也跑

### 0.4 验证
- [x] 0.4.1 `xtask test docs` 全绿。**存量 206 条死链未逐条修，改为棘轮基线**
      （`scripts/test/doc-link-baseline.txt`，204 条）——其中 121 条（59%）在 `docs/design/` 里，
      而那个目录正要被清空，现在修是白做。搬走一批基线少一批；阴性对照已做（新死链立刻判红）
- [x] 0.4.2 完整 `xtask test` 全绿
- [x] 0.4.3 三书 mdBook 构建：本地无 mdbook，由 CI 的 deploy-book PR 构建把关（`[ERROR]` 判红）

---

## 批 1 · internals/compiler

- [x] 1.1 `book/src/compiler/`(12) 按清单迁入 `internals/src/compiler/`（zbc/zpkg 除外 → 批 2 的 formats）
- [x] 1.2 `design/compiler/`(9)：`self-hosting` `binder-hierarchy` `scripting-charter` 迁入；
      `project.md` 字段全表 → `reference/manifest/`（**跨书，注意 SUMMARY 两边都要挂**）
- [x] 1.3 **逐节核实已完成**（User 2026-09-16 裁决「现在就做」）——结果见
      [batch1-architecture-verification.md](batch1-architecture-verification.md)：
      18 节里 8 节已被现有页覆盖、4 节机制已不存在（含 `Z42InterfaceType.TypeParams` /
      `ModifierMangling` —— **标识符能 grep 到但字段根本不存在**）、5 节是 C# 结构细节。
      ⚠️ **唯一有价值的「跨 zpkg IMPL 段传播」属 zpkg 格式 ⇒ 改由批 2 并入 `formats/zpkg.md`，
      原文件随批 2 删除**（本批删会让批 2 失去来源）
- [x] 1.7 顺手修 Scope 外事实错误（User 批准）：`compiler-z42c.md` 写「`src/compiler/` 下 5 个子包」，
      实际 `z42c.core` / `z42c.syntax` 已在 `src/libraries/`，`src/compiler/` 只剩 3 个
- [x] 1.4 删 `design/compiler/compilation.md`（`.zmod`/`.zbin` 机制已不存在，src 零引用）
- [x] 1.5 错误码切分：全量码表 → `reference/errors/codes.md`；「新增错误码」+ 分段规则 → `internals/compiler/error-codes.md`
- [x] 1.6 本批涉及路径的链接重指 + `xtask test docs` 绿

## 批 2 · internals/runtime + formats

- [x] 2.1 `book/src/runtime/`(21) 迁入 `internals/src/runtime/`
- [x] 2.2 建 `internals/src/formats/`：`zbc.md` `zpkg.md`（← book/compiler）+ `ir.md`（← design/runtime）
- [x] 2.3 `design/runtime/` 直迁 11 篇（含 4 篇前瞻，页头标「设计已定 / 未实施」——裁决 11）
- [x] 2.4 合并 7 篇（`gc` `safepoint` `diagnostics` `load-context` `native-ext-loader` `ir-specialization` `jit`）
      —— **book 是主干，design 只贡献增补**，逐篇按清单取舍
- [x] 2.5 **抢救判定**：`vm-architecture.md`(1212行) 直迁作 VM 总体架构页——它是唯一的全景页，
      book 21 页都是专题，没有替代品
- [x] 2.9 **（核实推翻清单）** 见 [batch2-merge-verification.md](batch2-merge-verification.md)：
      清单对 **gc / native-ext 两组的主干判断是反的**。`design/gc.md` 的 Safepoint 协议占 706 行、
      11 个子节逐个在当前 VM 命中（GC mode 37 文件 / write barrier 26 / debug invariants 73 /
      pause histogram 17 / heap snapshot 74 / finalizer 30），而 book 三页讲的是调参旋钮与 TLAB/SATB
      ⇒ **design 版作主干**。native-ext 同理（253 行含完整架构，book 版 182 行只有两个范式实例）
- [x] 2.10 `design/runtime/zbc.md` 的 **Minor changelog 表（79 行）迁入 `formats/zbc.md`** ——
      `version-bumping.md` 第 3 步明文要求每次 bump 往那张表加行，直接删会让该纪律失去落点；
      规范同步改指新位置
- [x] 2.6 删 `design/runtime/{zbc,zpkg}.md`（已迁移的历史壳）
- [x] 2.8 **（批 1 移交）** 把 `design/compiler/compiler-architecture.md` 的「跨 zpkg impl 块传播 —
      IMPL section + Phase 3 merge」（原文 439–517 行）并入 `formats/zpkg.md`，**同 PR 删除该文件**。
      依据见 [batch1-architecture-verification.md](batch1-architecture-verification.md)
- [x] 2.7 链接重指 + `xtask test docs` 绿

## 批 3 · reference/language（最大 —— 已拆成 3a / 3b 两个 PR）

> **拆分理由**：原计划一个 PR 装 55 篇输入 + 6 篇切片，不可评审。3a = 语言主体，3b = 切片与 internals 回流。

### 批 3a · 语言主体

- [x] 3a.1 `book/src/language/` 19 篇迁入 `reference/src/language/`
      （`type-conversion.md` → `conversions.md` 批 0 已完成；`generics.md` `member-accessors.md` 另计）
- [x] 3a.2 `design/language/` 直迁 + 合并 + 按源码重写
- [x] 3a.3 **`language-overview.md`(832行) 拆成 9 个主题页**
      `syntax` `types` `strings` `operators` `control-flow` `functions` `classes` `structs` `interfaces`
      —— ⚠️ **`unions` 取消**（裁决见下）
- [x] 3a.4 **新写 `memory-model.md`「所有权与内存模型」**（唯一无对应文件的占位）
- [ ] 3a.5 补 `reference/src/SUMMARY.md`（此前只挂了 2 页，40+ 页不可达）
- [ ] 3a.6 **清 reference → internals 的反向链接**（章程 §2.1 硬规则；book 搬来时带进 25 处 / 14 文件）
- [ ] 3a.7 链接重指 + `xtask test docs` 绿 + GREEN

### 批 3b · 切片与 internals 回流

- [ ] 3b.1 `interop.md`(720) 切片 —— ⚠️ **方向与原清单相反**（裁决见下）
- [ ] 3b.2 `object-protocol.md` / `boxing.md` / `closure.md` / `attributes.md` 切片
- [ ] 3b.3 `generics.md`(1521) 瘦身 —— 约 700 行可删（裁决见下）
- [ ] 3b.4 `conversions.md` 切掉「机制 / 实现」两段（已在 reference 但违反判据）
- [ ] 3b.5 链接重指 + `xtask test docs` 绿 + GREEN

> ⚠️ **格式 bump 刚合入后开的新 worktree 会撞种子窗口**（批 3b 实际踩到）：
> `install-z42.sh` 装的 nightly 是 bump **合并前**发布的，与源码的新常量 strict-pin 互不认，
> 第一轮 GREEN 必红在 `zpkg minor <旧> not supported (writer is at <新>)`。
> **这是假故障**（见 [[z42-worktree-seeding-false-failures]]），不是改坏了。
> 解法：复用该 bump 那个 PR 的 CI `toolchain-<os>` artifact overlay，
> 再 `cargo build --release --bin z42vm` + `export Z42_PORTABLE_VM=$PWD/artifacts/build/runtime/release/z42vm`。
> 等下一个 nightly 发布后自愈。

> ⚠️ **批 6 删 `docs/design/` 前必须先处理的两处硬依赖**（批 3b 发现）：
> 1. `src/runtime/tests/manifest_schema_validation.rs:22` **硬读**
>    `docs/design/compiler/manifest-schema.json` —— 删目录时这个测试会红。
>    该 schema 描述的整套 manifest 机制在自举后已无生产者也无消费者
>    （`NativeImportSynthesizer` / `ManifestSignatureParser` 全仓零命中）⇒
>    要么把 schema 落新家，要么连测试一并删。
> 2. 源码注释里的悬挂引用：`src/runtime/include/z42_abi.h` 头注释指
>    `docs/design/language/interop.md §3`；`src/runtime/README.md:72` 指
>    `docs/design/compiler/manifest-schema.json`。注释用**仓库根相对**路径，
>    重指时别加 `../`（见 [[z42-batch-rewrite-context-blindness]] 的教训）。

### 批 3 期间做出的裁决（推翻搬迁清单的部分）

| # | 裁决 | 依据 |
|---|---|---|
| A | **`interop.md` 的切分方向反过来**：C ABI 契约（§1 §3 §4 §5.1 §6 §7.2-7.3 §8.4）→ reference/embedding；三层架构 / 调用约定 / 内存 / manifest / §11 L1 `[Native]` → internals | `doc-system.md` §2.2 边界裁决表：「C ABI **契约**（宿主开发者也在用 z42）→ reference」。而 §11 的 `[Native("__name")]` 只有改 stdlib 的人才碰（名字必须已在 VM `BUILTINS` 表里，用户加不了） |
| B | **不建 `unions.md`**，判别联合留在 `pattern-matching.md:310-352` | `record` 关键字已删、全仓无 `union`/`variant`、roadmap 无此项。overview §11 整节编译不过，且它写的 `public` 恰好会**关掉**穷尽性检查（顶层默认 `internal` 才封闭） |
| C | **`static-abstract-interface.md`(626) 删除** | 准确内容已被 `reference/src/language/generic-constraints.md` 完整覆盖且更新。文档抬头「实现尚未开始」本身是错的（`src/tests/operators/static_abstract_operator.z42` 是跑着的 golden）。迁过去只会制造第二份会漂移的真相源 |
| D | **`grammar.peg` 移出 docs → `src/libraries/z42c.syntax/`**，加「非 SoT + 已知漂移清单」头 | 它头部声明的 SoT 门禁（C# parser + `dotnet test --filter GrammarSync`）已随自举整族消失，无人校验；抽查 12 条产生式**落空 7 条** |
| E | **`generics.md` 约 700 行可删**（47%） | 与 `generic-constraints.md`(522) / `generic-methods.md`(214) 重复约 300 行（原文自己在四处写「以 book 页为准」）+ 78 行自标 DEPRECATED + C# 触点 + roadmap/验证记录 |
| F | **`naming-conventions.md`(727) 一拆三** | 88% 是用户命名约定 → reference/conventions；本仓贡献者约定 → internals；包名规则 → `toolchain/z42-toml.md`（那是 manifest 字段约束，不是语言标识符规则） |
| G | **补齐 `appendix/error-codes.md`** | 章程规定 reference 附录是全量码表，实测只收录 **38 / 108**；且须逐条标注「有发射点」还是「已定义未接线」——多条死码（E0424 / E0420 / E0414）被文档写成生效规则 |

> **方法论教训（批 1/2 已记，批 3 再次验证）**：搬迁清单的「主干判定」是按文件名和新旧程度猜的，**不可照搬**。
> 批 3 对 28 条技术断言做字段级核实，**只有 11 条命中、10 条完全落空**。
> 落空集中在两类：① 自举把 C# 侧机制整族带走了；② 运行时表示重构过两轮而设计文档没跟。
> **凡文档声称「编译器会报 Exxxx」的，要么给得出发射点 file:line，要么标注「未实现」。**

## 批 4 · reference/stdlib

- [x] 4.1 `design/stdlib/` 包页 + `book/src/stdlib/`(4) 迁入 `reference/src/stdlib/`
- [x] 4.2 `time.md` 改写页头（`z42.time` 包已删，类型在 `z42.core/src/Time/`；命名空间仍是 `Std.Time`）
- [x] 4.3 `overview`→`architecture` / `organization` / `api-guidelines` → `internals/stdlib/`
- [x] 4.4 `json-serde` 切分：公开 API 并进 `reference/stdlib/json.md`；反射底座 / 分派轴 → `internals/stdlib/json-serde.md`
- [x] 4.5 `stdlib/roadmap.md` 并入 `docs/roadmap.md`（残余延后项只剩 5 个未开的包）
      ⚠️ **`README-template.md` 未动**——与 `readme-writing.md` §七「六段模板（唯一 SoT）」冲突，
      见 [batch4-verification.md 附录 B](batch4-verification.md) 第 1 条，**待 User 裁决**
- [x] 4.6 链接重指（`src/` 20 个文件含 `.z42` 注释 / `docs/roadmap.md` 16 条 / internals 5 处）+ `xtask test docs` 绿

### 超出原计划的补充（判断依据：reference 自己的判据「不读实现也能用对」）

原计划只搬 `design/stdlib/` 已有的页。实施时发现**用得最多的几个包根本没有设计页**，
若照原计划走，stdlib 参考会缺掉 `List` / `Dictionary` / `StringBuilder` / `Thread` /
`Console` / `File` / `Path` 这些天天用的东西。故新增 **7 页首次编纂**：

- [x] `collections-core.md`（`List<T>` / `Dictionary` / `HashSet` / `KeyValuePair` / `ReadOnlyCollection`）
- [x] `collections.md`（`Stack` / `Queue` / `LinkedList` / `PriorityQueue` / `SortedSet`）
- [x] `text.md`（`StringBuilder` / `Strings` / `Levenshtein`）
- [x] `threading.md`（`Thread` / `Channel` / `Mutex` / `RwLock` / `Timer`）
- [x] `io-file.md`（`Console` / `File` / `Directory` / `Path` / `Environment`）
- [x] `process.md`（`Process` / `ProcessHandle` / `Stdio` / `Ansi`）
- [x] `string.md`（`Std.String` 方法面全表；语法面仍归 `language/strings.md`）

### 本批的核实产出

见 [batch4-verification.md](batch4-verification.md)：**50+ 条实现缺口**，其中两条最重——
① `z42 run <单文件>` 加载不到跨包命名空间的后半边（三个组独立撞到，根因已定位）；
② native 压缩的**所有**错误路径自死锁 → VM 永久挂起（11 个发射点，有最小复现）。

## 批 5 · toolchain + devinfra

- [x] 5.1 `reference/toolchain/`：`cli-z42.md` / `cli-z42c-z42b.md` / `runtime-settings.md`（只取旋钮清单）
- [x] 5.2 `reference/embedding/c-abi.md`（裁决 12）：从 `internals/runtime/embedding.md` 切出 C ABI 契约面，
      internals 侧同步瘦身 591 → 367 行
- [x] 5.3 `internals/toolchain/`(8)：`z42b` `launcher` `repl` `deployment-model` `export`
      `platform-export` `workload-distribution` `editor-integration`
- [x] 5.4 `internals/testing/`(4)：`framework` `cross-platform` `embedded-app-run` `exec-profile-matrix`
- [x] 5.5 `internals/devinfra/`(14)：`docs/workflow/` **25 篇 → 6 页** + book/dev(6) + `artifacts-layout` + `test-pipeline`
- [x] 5.6 **抢救后删**（裁决 9）：`build-orchestrator.md` 的阶段管线 / `ICompiler` / hook 注入并进 `z42b.md`
      —— 核实后发现是**九**个阶段不是八个（漏了 `Preflight`）
- [x] 5.7 删 `design/testing/test-runner-bootstrap.md`（已核实：Rust runner 确已删，30 处 grep 命中逐行看过全是注释）
- [x] 5.8 链接重指 + `xtask test docs` 绿；死链棘轮基线 76 → **60 条**

### 超出原计划的补充

- [x] `internals/runtime/`：`gc-handle.md` / `stdlib-platform.md`（前几批清单提到但一直没做的遗留）
- [x] `reference/stdlib/`：`platform.md` / `gc.md`（`Std.Platform` / `Std.GC` / `GCHandle` 全无参考页）
- [x] `reference/testing.md`（搬迁清单里 reference 应有一页「测试」，至今没有）
- [x] **站点根改写**（原批 6.2）：`docs/book/` 内容已空但发布在站点根，SUMMARY 指向已删页会让
      **mdbook build 失败、deploy 断** ⇒ 就地改成三书分流索引
- [x] 重写 `docs/README.md`（原批 6.2）+ `docs/design/README.md`
- [x] 补 `internals` 三个部分的概览页（toolchain / testing / devinfra）
- [x] 删批 4 漏删的 `design/language/{reflection,string-builtins}.md`

### 门禁联动（真门禁，必须同批改）

`scripts/test/xtask_test.z42` 把 GREEN gate 的 stage 清单与 test-gate 页的 `gate-stages` 区逐项比对、
不一致判红。本批把该页搬到 `internals/devinfra/` ⇒ **同批改了脚本里的 4 处路径**
（1 处功能常量 + 2 处用户可见 `ConsoleError` 文案 + 1 处注释），并做了**双向实测**：
正向无报错；反向把页移走 → 报出预期错误 → 还原。

### 本批的核实产出

见 [batch5-verification.md](batch5-verification.md)。输入是**流程与架构描述**（不是 API 签名），
所以核实手法换成逐个路径 `ls`、逐个命令 `--help`、逐个 CI job 对 `.github/workflows/`。

`design/testing/testing.md`（1211 行）**几乎整篇是虚构的**——26 条断言落空。

## 批 6 · 收尾

- [x] 6.1 **`docs/design/` 整个目录删除**；`docs/workflow/` 已于批 5 删除
- [x] 6.2 站点根三书分流索引（批 5 提前做了——book 的 SUMMARY 指向已删页会让 mdbook build 失败）；
      重写 `docs/README.md`；游离文件 `philosophy.md` / `features.md` → `internals/src/`
- [x] 6.3 `workflow.md` 阶段 9 的**统一维护触发矩阵删除**，改为链 doc-system 三问
      （`code-organization.md` 里那份在更早的规范收口批已清）；
      连带修 `readme-writing.md` / `.claude/CLAUDE.md` 的三处引用
- [x] 6.4 全仓链接重指：`README.md` / `docs/roadmap.md` / `.claude/skills/` /
      `docs/agent/rules/{philosophy,readme-writing}.md` / internals 两页
- [x] 6.5 **grep 清零**：`docs/design/` 与 `docs/workflow/` 在三书 + agent/rules + `.claude/` +
      根 README 中**为 0**（`docs/spec/archive/` 与 `docs/spec/changes/` 的历史变更记录保留——
      那是留痕，改写等于篡改）
- [x] 6.6 **删 doc-system 的「过渡期」临时节**（章程自己写着「重构完成即删」）；
      `philosophy.md` 规则里的「不得再写进 docs/design/」改指新落点

### User 裁决落地（本批）

- [x] **「状态」字段只记未来**：`book-writing.md` §二 的 `**状态**: ✅ 已实现（0.3.x）` 改成
      **可选的「待办」行**——只记 ToDo / 已知缺口 / 后续迭代机会，没有就整行省掉。
      同步在 `doc-system.md` §6.3 补「禁的是回头看，不是向前看」的对照表
      （此前只说「不写历史」，导致各批把 Deferred 内容一并压掉了）
- [x] **README 模板合并**：`design/stdlib/README-template.md` 并入
      `readme-writing.md`——新增「待办」「依赖关系」两个可选段 + 库目录的两处细化
      （功能索引写成入口点、核心文件表加「类型」列），原文件删除

### 未做（需 User 定）

`docs/library_review.md`（2026-08-30 的一次性 stdlib 分析快照，结论已大部分被
`batch4-verification.md` 的实测覆盖或推翻）与 `docs/todo-list.md`（速记清单，部分条目已完成）
**未删也未合并**——删哪些、并哪些进 roadmap 需要 User 定。已在 `docs/README.md` 建「待归置」表登记。

## 批 7 · 其余门禁

- [ ] 7.1 `xtask test docs` 补齐：SUMMARY 完整性 / 页头「对齐」字段 / 命令面改名后旧名 grep 清零
      > **SUMMARY 完整性要查两件事**（批 3a 实测出的两类问题，缺一不可）：
      > ① SUMMARY 条目指向的文件存在 —— 不存在时 **mdbook 直接构建失败**；
      > ② 书里的 `.md` 都被 SUMMARY 挂上 —— **mdbook 不报错，但页面永远点不到**。
      > 批 3a 开工时 reference 下有 **40 页属于第 ② 类**（`conversions.md` 之外的全部语言页都不可达）。
      > 原型脚本见批 3a 的 PR 描述；注意条目标题可能含方括号（`` [`[Record]`] 与主构造器 ``），
      > 正则用 `[^\]]*` 会截断并把它误判成未挂载。
- [ ] 7.3 **`reference` / `learn` 里指向 `internals/` 的链接必须为 0**（章程 §2.1 硬规则）。
      批 3a 清理过一轮（25 处 / 14 文件，全是从 book 搬来时带的），但后续批次会再带进来
- [ ] 7.2 `internals/devinfra/test-gate.md` 同步新 stage

---

## 备注

**Scope 外发现（不在本次处理）**：
- `docs/spec/changes/` 有 **118 个未归档 change**，其中 **19 个已标 🟢**。建议单开 change 清理。
- ~~`.claude/rules/` 与 `docs/agent/rules/` 并存~~ → **已由 change `consolidate-agent-rules` 收口**（16 篇归一）。
  批 6 仍要删 `workflow.md` / `code-organization.md` 里的两份矩阵拷贝。

**本次搬迁的性质（别按「去重」理解）**：`docs/design/` 99 篇里 **72 篇在 book 中从未有过对应页（73%）**。
主体工作是**首次编纂**，不是搬运去重——这正是「顺带迁」两个半月只推进 2.5% 的原因。

**Scope 外偶发缺陷（本批实施中撞到，未修）**：`xtask test` 的 stdlib 段一次跑出
`Z42NetHttpServerThreadedTests.test_serve_threaded_handles_concurrent_clients:
Std.ThreadException: BrCond expects bool, got Null`，**同一用例随后 4 次复跑全过**。
本批只改文档与门禁脚本，不可能影响 VM 线程/GC ⇒ 判定为**既有偶发**。
签名与 `z42-park-window-null`（#617 已修：`Thread.Start` 捕获的环境在 spawn 窗口里没有 GC 根
⇒ 并发 HTTP 随机 `got Null`）**同族**，疑为残留或同族的另一处。已记入 memory，建议单开 change 追。

**已查实的三条事实**（删除判断的依据）：
1. C# 编译器已不存在（`find src -name '*.cs'` = 0）
2. `.zmod` / `.zbin` 在 src 零引用
3. `z42.time` 包已删，类型在 `z42.core/src/Time/`
