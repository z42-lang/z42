# Tasks: tidy-test-layout

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06
> 类型：`test` + `chore(toolchain)` —— 走[最小化模式](../../../.claude/rules/workflow.md)（无 proposal/spec/design）。

**变更说明：** 测试用例归位（库单测进库）、修好「被列出却从不运行」的 stdlib 单元发现漏洞、
把「Console.WriteLine + expected_output.txt」的机械 golden 改成 assert-only。

**原因：** User 提出三条：① 库的单元测试不该躺在 `src/tests/`；② `z42.ir` 下的用例没在跑；
③ `expected_output.txt` 太多、能不能少一个文件。查下来 ② 是一个**假绿**（不是没配好，是发现
规则有洞），顺带牵出两处已腐坏多时的断言。

**文档影响：** `src/tests/README.md`、`src/libraries/{z42.core,z42.ir,z42c.core,z42c.syntax}/README.md`、
`docs/workflow/testing/stdlib-tests.md`、`docs/design/compiler/self-hosting.md`。

---

## 1. 修根因：stdlib [Test] 单元发现的假绿

- [x] 1.1 `_discoverTestUnits`（`scripts/test/xtask_test_lib_units.z42`）去掉 dir 单元对
      `source.z42` 的要求，判据收敛为**「目录里有没有 `[Test]`」**——与 `_enumerateCorpus`
      （`xtask_test_embedded_corpus.z42` §2b，`test list` 用的那条）本就该是同一条。
      两条判据分家，是 `test list` 列得出、`test stdlib` 跑不到的直接原因；dir 单元本来就走
      合成 manifest 的 `**/*.z42` glob 编译，`u.SourceFile` 在这条路径上从未被读过。
- [x] 1.2 加门禁（`scripts/test/xtask_test_lib.z42`）：**点名某库、其 `tests/` 下有 `.z42`
      源却发现不到任何 unit → 判红**。刻意收窄三处，只对真缺陷开火：仅点名（无名扫库照旧）、
      源必须真实存在（`z42.build` 这类无 `tests/` 的库不受影响，否则 `test changed` 会误报）、
      无 `-k` 过滤（过滤没命中是过滤自己的事）。
- [x] 1.3 三个受影响库的用例改成 flat 单文件、删 9 份手写 `.z42.toml`：
      `z42.ir/tests/{smoke,depindex,zpkg}.z42`、`z42c.core/tests/{diag,features}.z42`、
      `z42c.syntax/tests/{lexer,decl,parser,stmt,incomplete_at_eof,dump}.z42`。
      与其余 20+ 个 stdlib 库同形，也不再需要每个目录配一份 manifest。

### 首次运行抓到的两处腐坏（这正是假绿的代价）

- [x] 1.4 `z42.ir/tests/zpkg.z42::test_write_packed_header` 把 zpkg minor 钉成字面量
      `0x21`(33)，实际已是 43——**落后 10 个版本**。改成从 `ZpkgWriterZ.Major/.Minor` 推导，
      断言回归到「头部**形状**」这个它真正该守的契约，同时从
      [version-bumping.md](../../../.claude/rules/version-bumping.md) 的手工同步清单上除名。
- [x] 1.5 `z42c.syntax/tests/stmt.z42` 的 4 条 `DeclarationInStatement` 断言曾报
      `FieldGet: ... got Null`。**排查结论：不是代码问题**，是我给 worktree 供的种子
      artifacts 过旧（早于 #471）。用当前源重建工具链后 21/21 全绿，源码未改。

## 2. 归位：库的单元测试进库

- [x] 2.1 `src/tests/strings/` 里的 String **库行为**用例全部删除——
      `string_builtins` / `string_methods` / `string_script` / `string_params_methods` /
      `string_byte_length`，逐条比对后确认是
      `z42.core/tests/string_methods.z42` 的**真子集**。三处独有断言合入该文件：
      2 字节 + 4 字节 + 混合的 `ByteLength`、4 参 `Concat`（超原三元固定 arity）、
      `FromChars(new char[0])`。
- [x] 2.2 `strings/string_static_methods` + `string_params_methods` 里**唯一不可替代**的部分
      ——「**小写关键字**作静态调用接收者」（`string.Join` / `int.Parse` / `double.Parse`，
      `[Test]` 单测里写的都是 `String.Join`，没人验小写形态）——提成
      `src/tests/types/keyword_type_statics.z42`。这是语言侧的关键字→类型解析，留在 `src/tests`。
- [x] 2.3 `src/tests/basic/assert.z42` → `z42.core/tests/std_assert/source.z42`。
      **刻意保持 Main-based golden 形态**：`[Test]` 文件必须 `using Std.Test`，那会把裸名
      `Assert` 绑到 `Std.Test.Assert`，被测的 `Std.Assert` 反而没人测。
- [x] 2.4 删 `src/tests/types/enum_parse.z42`——`z42.core/tests/enum_parse_isdefined.z42`
      逐条覆盖且更全（含 gap 值 IsDefined 反例）。
- [x] 2.5 归属判据写进 `src/tests/README.md`：**「这条断言在描述谁的契约」**，而不是
      「实现落在哪一层」。`Std.Reflection` / GC / `Std.Runtime` 这些**门面在库、实现在 VM**
      的用例因此**留在 `src/tests/`**——它们验的是 VM 行为，搬进库反而是二次错位。

## 3. 削 expected_output.txt（机械可转的那批）

- [x] 3.1 转 assert-only 并删侧车（15 个）：`optimization/` 全部 8 例、`osr/loop_once_called`、
      `types/box_unbox`、`gc/{composite_ref_weak_mode,weak_subscription_lapsed,weak_subscription_alive}`、
      `gc/{gc_softhandle_basic,gc_softhandle_pressure,gc_softhandle_strong_wins,gc_oom_exception}`。
      前 11 个顺带 dir → flat（少一层目录）；后 4 个带 `interp_only` 标记，保留目录形态。
- [x] 3.2 `strings/raw_string_basic` dir → flat assert-only。
- [x] 3.3 **改写时补强，而非等价搬运**：
      - 「死分支不该执行」原先靠「输出里没有那一行」间接表达 → 改计数器 +
        `Assert.Equal(0, hits)`，变成正面证明（`const_dead_branch`、两个 weak GC 用例）。
      - `loop_alloc_reuse_carried` 补 `Assert.Equal(4, len)`：链表若被错误复用，长度先崩。
      - 原 golden 唯一在验的 `ToString` 形态（`bool`→"true"、`double`→"3.14"、`char`→"z"、
        `long`→"1249975000"）逐条留住，没有随侧车一起丢。
      - `gc_oom_exception` 先恢复堆上限再断言——1 字节 strict-OOM 下让断言机制受制于它
        正在测的那个限制，是自找的不稳定。
- [x] 3.4 `src/tests/README.md` 立规矩：**默认 assert-only**；侧车只在 stdout 本身就是被测
      契约时才该有（异常栈迹、`Console` 格式化、多 exe 输出顺序、REPL 会话）。判据：
      「改写成断言会不会丢掉只有 stdout 能表达的东西？」

### 明确**不动**的（保留侧车是对的）

`exceptions/`（栈迹文本）、`delegates/` + `interfaces/interface_event`（多播调用顺序）、
`cross-zpkg/`、`multi-exe/`（跨 exe 输出拼接顺序）、`z42.scripting/`（REPL 会话记录）、
`z42.test/test_runner`（TAP 输出本身）、`basic/zlib_format`（另有 `emit_format.txt` 侧车，
删一个不省事）。这些的 stdout 就是契约，改成断言是**丢覆盖**，不是清理。

## 4. 文档同步

- [x] 4.1 `src/tests/README.md`：归属判据 + assert-only 优先 + `strings/` 类别重新定义。
- [x] 4.2 `docs/workflow/testing/stdlib-tests.md`：新增「什么算一个 unit」（两种形态 + 判据 +
      这次踩的坑），加新测试段补「该不该写在这里」。
- [x] 4.3 四个库 README 的测试段：`z42.core`（新增「如何测试验证」，讲清两种形态怎么选）、
      `z42.ir` / `z42c.core` / `z42c.syntax`（**订正跑法**：后两个原写「经 `xtask test compiler`」，
      而 `test compiler` 只扫 `src/compiler/<member>/tests/`，从来扫不到 `src/libraries/`）。
- [x] 4.4 `docs/design/compiler/self-hosting.md`：受限写法表里 `zpkg_tests` → `z42.ir/tests/zpkg.z42`。

## 5. 验证

- [x] 5.1 `xtask test` **全绿**（10 stage，3m02s）。
- [x] 5.2 三个库首次真跑：z42.ir 12 / z42c.core 11 / z42c.syntax 114 个 `[Test]`，全通过。
- [x] 5.3 门禁行为逐条实测：无 `tests/` 的库不误报、`-k` 未命中不误报、
      构造一个「有源无 unit」的目录 → 如期判红。
- [x] 5.4 转写过的用例 interp + jit 双模式全绿（optimization 16、gc 22、types 124、strings 4、osr 2）。

## 备注

- **未跟进（不在本次范围）**：`docs/spec/changes/migrate-stdlib-to-params/` 与
  `fix-crosspkg-static-ns-collision/` 的 tasks/proposal 里引用了本次移动的用例路径。
  那是**别人在制品**的记录文件，且后者的引用在本次之前就已过时（还写着
  `src/compiler/z42c.ir/`）——不越界改。归档时由各自的 owner 顺手更即可。
- **未跟进（本次刻意不做）**：`src/tests/` 里 `reflection/`、`gc/`、`types/*_reflect*`
  这批看起来「像库测试」的用例**没有搬**。理由见 2.5：它们的门面在 `Std.Reflection` /
  `Std.Runtime`，行为却整个由 VM 实现，搬进库会把「VM 特性测试」错标成「库 API 测试」。
