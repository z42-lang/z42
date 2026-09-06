# src/tests/ — 中央 VM 端到端测试集

## 职责

按特性分类的 z42 **VM 端到端**测试集（VM 真跑、interp + JIT 两轮），对标
[dotnet/runtime/src/tests/](https://github.com/dotnet/runtime/tree/main/src/tests)。

支持两种用例形态（dual-mode discovery，2026-05-08）：
- **Dir 模式** — `<category>/<name>/` 目录含 `source.z42` + 可选 sidecar 文件（适合需要 `features.toml` / `expected_output.txt` 等 sidecar 的用例）
- **Flat 模式** — `<category>/<name>.z42` 单文件（仅 assert-only run 用例：用 `Std.Assert` 抛异常表达失败，期望空 stdout，无任何 sidecar）

不放在这里：
- 编译器单元测试（语义层）→ [src/compiler/z42c.semantics/tests/](../compiler/z42c.semantics/tests/)
- 编译器单元测试（语法层）→ [src/libraries/z42c.syntax/tests/](../libraries/z42c.syntax/tests/)
- **期望编译报错的用例** → 同上（`[Test]` + `SemanticDump`，见下方说明）
- VM Rust 单元测试 → [src/runtime/src/](../runtime/src/) 同模块的 `*_tests.rs`
- VM Rust 集成测试（zbc_compat / native interop / manifest schema）→ [src/runtime/tests/](../runtime/tests/)
- stdlib 库本地测试 → [src/libraries/<lib>/tests/](../libraries/)

> **归属判据（tidy-test-layout，2026-09-06）**：本目录测的是**语言 / VM 特性**——
> 语法、类型系统、派发、GC、优化 pass、自举格式。一个用例若在测**某个库的 API 行为**
> （`String.Trim`、`Enum.Parse`、`List<T>`、`Std.Assert` …），它属于那个库的
> `src/libraries/<lib>/tests/`，不属于这里，哪怕该 API 由 VM builtin 实现。
> 判据是「这条断言在描述谁的契约」，不是「实现落在哪一层」。

## 类别

| 类别 | 内容 |
|------|------|
| `basic/` | 基础功能：hello / fibonacci / arrays / namespace / assert dogfood |
| `exceptions/` | try/catch/finally / 嵌套 / stack trace / exception subclass |
| `generics/` | 泛型函数 / 类 / 约束 / 实例化 / interface dispatch |
| `inheritance/` | virtual / abstract / multilevel / implicit object base |
| `interfaces/` | multi-interface / 属性 / IComparer / event |
| `delegates/` | delegate / multicast / event / nested |
| `closures/` | lambda / closure / local function |
| `gc/` | GC cycle / collect / weak ref / weak subscription |
| `types/` | enum / struct / record / typeof / is/as / nullable / numeric aliases / char |
| `control_flow/` | switch / do-while / null-coalesce / null-conditional / loop control / nested |
| `operators/` | bitwise / 增量 / parse / postfix / 逻辑 / 比较 / 重载 |
| `refs/` | ref / out / in / nested ref |
| `classes/` | class / namespace / access / static / auto-property / ctor / indexer |
| `strings/` | **语言侧**的字符串字面量：raw string `"""…"""` / 插值 / 拼接。String 的**库行为**（Length·Trim·Split·Join·Format…）归 [z42.core](../libraries/z42.core/tests/string_methods.z42)，不在这里 |
| `cross-zpkg/` | 多 zpkg 端到端（target / ext / main 三方协作；由 `z42 xtask.zpkg test cross-zpkg` 跑） |

> **期望编译报错的用例不在本目录**：写成 `z42c.semantics` 自己的 `[Test]` 单测
> （`src/compiler/z42c.semantics/tests/typecheck/`，用 `SemanticDump.FirstErrorCode` /
> `FirstErrorMessage` / `FirstErrorPos` / `ErrorCount` 断言），由 `xtask test compiler` 驱动。
> 参考 `undefined_type_tests.z42` / `constraint_tests.z42`。
>
> 历史：负例语料曾在本目录 `errors/`，2026-05-12 搬进 C# 测试项目 `z42.Tests/Fixtures/`，
> 2026-06-26 C# 编译器移除时随整个测试项目一起蒸发——自举迁移只搬了「能编过」的正例，
> 导致一批诊断静默退化（详见 change `complete-where-constraints`）。
>
> ⚠️ **仍缺口**：`SemanticDump` 只覆盖**单文件语义**诊断。跨包 / 多文件的期望报错
> （E0404 跨包 internal 等）今天仍靠手工验证 fixture + README 描述步骤，**没有自动门**。

## 用例文件约定

### Dir 模式（`<category>/<name>/`）

| 文件 | 何时存在 | 含义 |
|------|---------|------|
| `source.z42` | 必须 | z42 源码 |
| `source.zbc` | run / parse | 由 `z42 xtask.zpkg regen` 生成，按组件镜像落 `artifacts/build/tests/<rel>/source.zbc`（**不与源同处**，gitignored，不污染 src）。例外：`zbc-format/*/source.zbc` 是 check-in 字节基线，就地重写 |
| `source.zasm` | 可选 | ZASM 调试文本 |
| `expected_output.txt` | run | stdout 期望。**默认不要有这个文件**——见下方「先写 assert-only」。空文件 = 删除；缺失 = assert-only 模式（用例靠 `Std.Assert` 抛异常表达失败，期望空 stdout）|
| `expected.zasm` | parse | IR ZASM 期望 |
| `features.toml` | 可选 | LanguageFeatures override |
| `interp_only` | 可选 marker | 跳过 JIT 模式 |

> **先写 assert-only，别默认加 `expected_output.txt`**（tidy-test-layout，2026-09-06）。
> 把断言写成 `Assert.Equal(...)` 而不是「打印一行、再拿侧车比对」有三个好处：期望值就在
> 断言旁边（不必在两个文件间来回对照）、失败信息直接指出哪一条不符（而不是一份 diff）、
> 少一个文件。**「某个分支不该被执行」这类否定命题尤其要用断言**：golden 只能靠「输出里
> 没有那一行」间接表达，计数器 + `Assert.Equal(0, hits)` 才是正面证明。
>
> 侧车只在 **stdout 本身就是被测契约**时才该存在，例如：异常栈迹文本、`Console` 对某类型的
> 格式化、多 exe 的输出顺序、REPL 会话记录。判据：「把它改写成断言，会不会丢掉只有 stdout
> 能表达的东西？」不会 → 就该是 assert-only。

### Flat 模式（`<category>/<name>.z42`）

仅适用于 assert-only run 用例。无任何 sidecar — 期望空 stdout，使用 `LanguageFeatures.Phase1` 默认配置。
对应的 `<name>.zbc` 由 `z42 xtask.zpkg regen` 生成（不入库）。

**何时使用 Flat 模式**：用例只调用 `Assert.*`（无 `Console.WriteLine` 输出对照），且不需要 features 覆盖、emit 格式覆盖、interp_only 标记或任何其他 sidecar。

## 添加新测试

按以下顺序判断归属（先到先得）：

1. **库 API 行为** → `src/libraries/<lib>/tests/<name>[.z42]`
2. **期望编译报错** → `src/compiler/z42c.semantics/tests/typecheck/<topic>_tests.z42`
   （`[Test]` 单测 + `SemanticDump.FirstErrorCode`，**不放本目录**；见上方说明）
3. **仅 ZASM 匹配** → `src/libraries/z42c.syntax/tests/dump/`（parser dump 单测）
4. **跨多 zpkg** → `src/tests/cross-zpkg/<name>/`
5. **其他 VM/编译器特性**：
   - 用 `Console.WriteLine` 测打印行为 / 需要 sidecar → `src/tests/<category>/<name>/source.z42` + sidecars（dir 模式）
   - 仅用 `Assert.*` 测计算 / 控制流，无 sidecar → `src/tests/<category>/<name>.z42`（flat 模式）
   - 不确定类别归 `basic/`

完整规则见 [docs/design/testing/testing.md](../../docs/design/testing/testing.md)。

## 运行

```bash
z42 xtask.zpkg test vm          # 全部 run 用例（interp + jit；不含 cross-zpkg）
z42 xtask.zpkg test cross-zpkg  # 仅 cross-zpkg
z42 xtask.zpkg test compiler    # z42c 自举 + 编译器 [Test] 单测（含期望报错的负例）
```

或一把跑全 GREEN：`z42 xtask.zpkg test`。
