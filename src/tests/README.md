# src/tests/ — 中央 VM 端到端测试集

## 职责

按特性分类的 z42 **VM 端到端**测试集（VM 真跑，interp + JIT 两轮）。

两种用例形态：
- **Dir 模式** — `<category>/<name>/` 目录含 `source.z42` + 可选 sidecar 文件
- **Flat 模式** — `<category>/<name>.z42` 单文件（仅 assert-only 用例：用 `Std.Assert` 抛异常表达失败，期望空 stdout，无 sidecar）

不放在这里：
- 编译器单元测试（语义层）→ [src/compiler/z42c.semantics/tests/](../compiler/z42c.semantics/tests/)
- 编译器单元测试（语法层）→ [src/compiler/z42c.syntax/tests/](../compiler/z42c.syntax/tests/)
- **期望编译报错的用例** → 同上（`[Test]` + `SemanticDump`，见下文）
- VM Rust 单元测试 → [src/runtime/src/](../runtime/src/) 同模块的 `*_tests.rs`
- VM Rust 集成测试（zbc_compat / native interop / manifest schema）→ [src/runtime/tests/](../runtime/tests/)
- stdlib 库本地测试 → [src/libraries/<lib>/tests/](../libraries/)

> **归属判据**：本目录测的是**语言 / VM 特性**——语法、类型系统、派发、GC、优化 pass、自举格式。
> 一个用例若在测**某个库的 API 行为**（`String.Trim`、`Enum.Parse`、`List<T>`、`Std.Assert` …），
> 它属于那个库的 `src/libraries/<lib>/tests/`，哪怕该 API 由 VM builtin 实现。
> 判据是「这条断言在描述谁的契约」，不是「实现落在哪一层」。

## 功能索引

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
| `optimization/` | 编译期优化 pass 的端到端行为。**用例必须带 `opt_all` sidecar**——不带就是在测「优化关着时」的行为，等于没测 |
| `operators/` | bitwise / 增量 / parse / postfix / 逻辑 / 比较 / 重载 |
| `refs/` | ref / out / in / nested ref |
| `classes/` | class / namespace / access / static / auto-property / ctor / indexer |
| `attributes/` | attribute 机制：字段 / 方法 attribute、`methodof`、`[Record]` 值语义 |
| `const/` | `const` 编译期常量 |
| `ctor-reflection/` | 构造器反射 |
| `generic-methods/` | 泛型方法：类型实参 / lambda 推断 / 默认值 / 多型参 / `new T()` / 静态 |
| `generic-method-invoke/` | 泛型方法的反射调用 |
| `named-args/` | 命名实参（含 `params`）|
| `null_checks/` | 空值检查标记（`?`）的逃生口等 |
| `osr/` | on-stack replacement（循环热点换层）|
| `params/` | `params` 可变参数：展开 / 常规 / `object` 混合 |
| `partial-types/` | `partial` 类型与方法：class / interface / record / static / protocol 重载 |
| `pattern-matching/` | 模式匹配：常量 / 位置解构 / 属性 / 绑定 / 穷尽性 |
| `reflection/` | 反射：`Activator` / `methodof` / 方法调用 / LoadContext |
| `static-ctor/` | 静态构造器触发时机 / 至多一次 / 失败终态 / 字段初始化顺序 |
| `structs/` | struct 的 ToString 路径等 |
| `tuples/` | 元组 |
| `user-conversions/` | 用户定义 `implicit` / `explicit` 转换 |
| `strings/` | **语言侧**的字符串字面量：raw string `"""…"""` / 插值 / 拼接。String 的**库行为**（Length·Trim·Split·Join·Format…）归 [z42.core](../libraries/z42.core/tests/string_methods.z42) |

> **仓库根 `examples/`** 不是测试语料：它是学习手册的配套工程，由 `./xtask test docs examples`
> 按会话脚本逐条运行校验（见 [test-gate](https://github.com/z42-lang/z42/blob/main/docs/internals/src/devinfra/test-gate.md)）。
> 语言 / VM 特性的覆盖一律写在本目录。

## 期望编译报错的用例

不在本目录。三种写法，按需选：

1. **单包负例**——写成 `z42c.semantics` 的 `[Test]` 单测（`src/compiler/z42c.semantics/tests/typecheck/`），
   用 `SemanticDump.FirstErrorCode` / `FirstErrorMessage` / `FirstErrorPos` / `ErrorCount` 断言，
   由 `./xtask test compiler` 驱动。参考 `typecheck/undefined_type/undefined_type_tests.z42` / `typecheck/constraint_tests.z42`。
2. **跨包负例（单测）**——`IrDump.ExtractExports` 把源码合成 `ExportedModuleZ`（= 一个依赖包的导出面），
   配上**包名**喂 `ImportedSymbolLoader.Load`，在内存里造出任意跨包形状，再用
   `IrDump.BuildPackage(..., imported, ...)` 编消费方并断言诊断码。范例见
   `z42c.semantics/tests/typecheck/crosspkg_duplicate/`。
3. **跨包负例（fixture）**——在 [z42c.pipeline 的 fixtures](../compiler/z42c.pipeline/tests/fixtures/cross-zpkg/) 里放
   `expected_build_error.txt`（**不是** `expected_output.txt`），内容为 stderr 必须包含的子串；
   `main` 编过了判红，错误文本对不上也判红。走真实三包 + 真 .zpkg 元数据，慢但覆盖真实接线。
   范例见 `cross-zpkg/dup_fqn_crosspkg/`、`class_internal_access`、`interface_internal_access`。

## 用例文件约定

### Dir 模式（`<category>/<name>/`）

| 文件 | 何时存在 | 含义 |
|------|---------|------|
| `source.z42` | 必须 | z42 源码 |
| `source.zbc` | run | 由 `./xtask build test` 生成，按组件镜像落 `artifacts/build/tests/<rel>/source.zbc`（不与源同处，gitignored） |
| `expected_output.txt` | run | stdout 期望。**默认不要有这个文件**——见下方「先写 assert-only」。缺失 = assert-only 模式 |
| `interp_only` | 可选 marker | 跳过 JIT 模式 |
| `opt_all` | 可选 marker | **按真实 release 全优化编**（`z42c --emit-zbc --opt-all`）。默认的 `--emit-zbc` 优化集关掉了逃逸分析 / 内联 / loop-alloc-reuse（它们会改 golden 字节），所以不加这个 marker 的用例跑不到那几个 pass。测优化 pass 的用例必须加 |

> **先写 assert-only，别默认加 `expected_output.txt`**。
> 把断言写成 `Assert.Equal(...)` 而不是「打印一行、再拿侧车比对」有三个好处：期望值就在
> 断言旁边、失败信息直接指出哪一条不符（而不是一份 diff）、少一个文件。
> **「某个分支不该被执行」这类否定命题尤其要用断言**：计数器 + `Assert.Equal(0, hits)` 才是正面证明。
>
> 侧车只在 **stdout 本身就是被测契约**时才该存在，例如：异常栈迹文本、`Console` 对某类型的
> 格式化、多 exe 的输出顺序、REPL 会话记录。判据：「把它改写成断言，会不会丢掉只有 stdout
> 能表达的东西？」不会 → 就该是 assert-only。

### Flat 模式（`<category>/<name>.z42`）

以 assert-only 用例为主。sidecar 为同名前缀文件：marker（`<name>.interp_only` / `<name>.opt_all`）与可选的 `<name>.expected_output.txt`（仅 stdout 本身是被测契约时）；无期望文件 = 期望空 stdout。对应的 `<name>.zbc` 由 `./xtask build test` 生成（不入库）。

适用条件：用例只调用 `Assert.*`，且不需要 features 覆盖或 emit 格式覆盖。

## 添加新测试

**先判归属**：用例该不该放这里、不放这里该放哪，以
[测试用例组织规范](https://github.com/z42-lang/z42/blob/main/docs/internals/src/devinfra/test-layout.md) 为准（唯一权威）。
本目录只收语言 / VM 特性；**新增类别要登记**进该页的「语言类别」清单，否则 `./xtask check layout` 判红。

确定放这里之后：
- 用 `Console.WriteLine` 测打印行为 / 需要 sidecar → `src/tests/<category>/<name>/source.z42` + sidecars（dir 模式）
- 仅用 `Assert.*` 测计算 / 控制流，无 sidecar → `src/tests/<category>/<name>.z42`（flat 模式）
- 不确定类别归 `basic/`

runner 怎么发现、执行用例见 [测试框架与 runner](https://github.com/z42-lang/z42/blob/main/docs/internals/src/testing/framework.md)。

## 如何测试验证

```bash
./xtask test e2e                  # 本目录全部用例（interp + jit），随后跑 cross-zpkg / multi-exe 夹具
./xtask test e2e --dir <category> # 只跑一个类别
./xtask test compiler             # z42c 自举 + 编译器 [Test] 单测（含期望报错的负例）
```

或一把跑全 GREEN：`./xtask test`。
