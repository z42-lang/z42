# Tasks: 文档三书重构

> 状态：🟡 进行中 | 创建：2026-09-16
> 分 8 批，每批一个 PR。**User 裁决：穿插在功能开发之间推进**，不连续做完。
> 逐文件搬迁清单见 [migration-manifest.md](migration-manifest.md)；角色与规范见 [design.md](design.md)。

## 进度概览
- [x] **批 0 · 立宪**（PR 待合）
- [x] 批 1 · internals/compiler（PR 待合）
- [x] 批 2 · internals/runtime + formats（PR 待合）
- [ ] 批 3 · reference/language
- [ ] 批 4 · reference/stdlib
- [ ] 批 5 · toolchain + devinfra
- [ ] 批 6 · 收尾（删空壳 + 链接重指 + 游离文件）
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

## 批 3 · reference/language（最大）

- [ ] 3.1 `book/src/language/`(22) + `book/compiler/type-conversion.md` 迁入 `reference/src/language/`
- [ ] 3.2 `design/language/` 直迁 + 合并（按清单 20 余项）
- [ ] 3.3 **`language-overview.md`(832行) 拆成 ~10 个主题页**（裁决 10）：
      `syntax` `types` `strings` `operators` `control-flow` `functions` `classes` `structs` `interfaces` `unions`
- [ ] 3.4 填 SUMMARY 的语言类空占位；**新写「所有权与内存模型」**（唯一无对应文件的占位）
- [ ] 3.5 切片：`interop` §1–§10、`object-protocol` 实现节、`boxing` 实现节、`closure` 档C节 → internals
- [ ] 3.6 链接重指 + `xtask test docs` 绿

## 批 4 · reference/stdlib

- [ ] 4.1 `design/stdlib/` 15 个包页 + `book/src/stdlib/`(4) 迁入 `reference/src/stdlib/`
- [ ] 4.2 `time.md` 改写页头（`z42.time` 包已删，类型在 `z42.core/src/Time/`）
- [ ] 4.3 `overview` `organization` `api-guidelines` → `internals/stdlib/`
- [ ] 4.4 `json-serde` 切分：公开 API → reference；反射底座/分派轴 → internals
- [ ] 4.5 删 `README-template.md`（移入 agent/rules）、`stdlib/roadmap.md`（并入 docs/roadmap.md）
- [ ] 4.6 链接重指 + `xtask test docs` 绿

## 批 5 · toolchain + devinfra

- [ ] 5.1 `reference/toolchain/`：`cli/z42.md` `cli/z42c-z42b.md` `cli/runtime-settings.md`（只取旋钮表）
- [ ] 5.2 `reference/embedding/`（裁决 12）：`design/runtime/embedding.md` 的 C ABI 契约面
- [ ] 5.3 `internals/toolchain/`：`z42b` `launcher`(主干) `repl`(**design 是主干**) `deployment-model`
      `export` `platform-export` `workload-distribution` `editor-integration`
- [ ] 5.4 `internals/testing/`：`framework` `cross-platform` `embedded-app-run` `exec-profile-matrix`
- [ ] 5.5 `internals/devinfra/`：book/dev(6) + `test-pipeline` + `artifacts-layout` + **`docs/workflow/` 全部 25 篇**
- [ ] 5.6 **抢救后删**（裁决 9）：`build-orchestrator.md`(190行) 的八相位 / `ICompiler` in-process /
      hook 注入并进 `internals/toolchain/z42b.md`，同 PR 删原文件
- [ ] 5.7 删 `design/testing/test-runner-bootstrap.md`（Rust runner 已删）
- [ ] 5.8 链接重指 + `xtask test docs` 绿

## 批 6 · 收尾

- [ ] 6.1 删 `docs/design/` 与 `docs/workflow/` 空壳（含各自 README）
- [ ] 6.2 站点根三书分流索引（从批 0 挪来）；游离文件：`features.md` → internals；删 `library_review.md` / `todo-list.md`；重写 `docs/README.md`
- [ ] 6.3 `../../../agent/rules/workflow.md` 阶段 9 与 `code-organization.md`：**删矩阵拷贝**，改链接总纲
- [ ] 6.4 **全仓链接重指**（裁决 8：脚本批量 + 人工抽查）：
      `src/**/README.md`(76) / `.claude/` / `scripts/README.md` / 根 `README.md` / `docs/learn/` / `docs/roadmap.md`
- [ ] 6.5 grep 清零：`docs/design/` 与 `docs/workflow/` 字样在全仓为 0
- [ ] 6.6 `philosophy.md` 的「延后记 `docs/design/<dir>/`」改指 internals（**本次顺带解决那条一直绕过去的冲突**）

## 批 7 · 其余门禁

- [ ] 7.1 `xtask test docs` 补齐：SUMMARY 完整性 / 页头「对齐」字段 / 命令面改名后旧名 grep 清零
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
