# Proposal: 给包一个**角色**维度（运行时 / 编译期 / 契约）

> 状态：**DRAFT，待 User 裁决**。起因是「用户写不了 generator」这个具体缺口，但根因不是打包
> 漏项，而是**包模型里缺一个维度**。

## Why

### 直接症状：Generator 契约用户够不着

实测（2026-09-23，端到端）：写一个 `ModuleGenerator` 库，`[dependencies]` 写
`"z42c.semantics"`，得到 `E0443: undefined type: ModuleGenerator`。
把 `z42c.semantics.zpkg` 丢进 SDK `libs/`，**同一份源码立刻编过、装进消费方、跑出结果**。

所以功能是齐的（契约、loader、有界多轮引擎全部可用），**只有「那个 zpkg 不在用户能解析到的
目录里」这一件事**挡着。Analyzer 那半能用，纯粹因为它的契约恰好住在 `z42c.syntax`，而
`z42c.syntax` 早先因为**另一个**原因（`converge-z42-syntax-lib`，为 scripting/playground 共享
前端）被搬进了 `src/libraries/`。

### 根因：一个物理位置在同时决定三件不相干的事

今天**没有**任何一处声明「这个包是什么角色」。角色是从**物理位置**推出来的：

| 想表达的事 | 今天的判据 | 出处 |
|---|---|---|
| 进不进 SDK `libs/`（= 用户能不能 `[dependencies]` 到它） | 是不是 `src/libraries/z42.workspace.toml` 的 `default-members` | `xtask_stdlib.z42:105`（列表单一真相）→ `_pkgCopyLibs` 纯 glob |
| publisher 要不要把它 bundle 进 apphost payload | **在不在 shipped `libs/`** | `builder_publish.z42:576` |
| 递归穿透时算不算「框架、到此为止」 | **在不在 `src/libraries/` 目录下** | `builder_publish.z42:568` |

三个判据、三种含义，却只有**一个**旋钮（把目录搬到哪）能拨动它们。后果已经写在代码注释里：

> 「**不能**用「是否 src/libraries 成员」——否则 launcher 的 `z42.workload.*`（非 src/libraries、
> 也不在 libs/）会漏拷。」
> 「……这样才能穿透「**已 ship 进 libs/ 却越界依赖非框架**」的 lib（如 `z42.scripting` 在 libs/
> 却依赖 `z42c.*`），触达其隐藏的传递非框架依赖。」
> —— `builder_publish.z42:546-551`

这段注释是本提案最好的证据：**判据已经在打补丁，而且作者知道它在打补丁**。`z42.scripting`
被形容为「越界」，`z42.workload.*` 被形容为「漏拷」——两者都不是 bug，是**角色表达不出来**。

`z42.workspace.toml` 里那条注释同样是：

> 「**非** Std/z42.* 标准库 API 面——**仅恰好与 stdlib 同处 build+ship**。」

「仅恰好」四个字就是缺失维度的自白：`z42c.core` / `z42c.syntax` 需要被 ship，但**不该**被当成
标准库 API 面；今天没有办法既要前者又不要后者，只能写注释请后人别误会。

### 为什么不能「把 z42c.semantics 也搬进 src/libraries/」了事

那正是今天唯一可用的杠杆，但它会**同时**触发上面三件事：

1. ✅ 进 `libs/` —— 这是我们要的；
2. ⚠️ 被 publisher 判为 framework，**从每个 apphost 的 payload 闭包里消失**；
3. ⚠️ 被 `_pubBundleProjectDeps` 判为「真·stdlib 成员 ⇒ 不递归」，而 `z42c.semantics` 依赖
   `z42.io` / `z42.threading` / `z42.ir`，与「只依赖框架」的前提**不符**；
4. ⚠️ 语义上宣告「编译器的整个语义层是标准库的一部分」——这是个大得多的承诺，且与
   「尽量减少标准库」（`converge-z42-syntax-lib` 明确写下的减法目标）相反。

⇒ 为了让 generator 可写而付上面 2/3/4，是**用错误的旋钮拨对一件事**。

## What：引入 `role`

给 `[project]` 增加 `role`（名字待定），与既有的 `kind`（lib/exe）正交：

| role | 含义 | 能被谁引用 | 链入产物 | 落在哪 |
|---|---|---|---|---|
| `runtime`（默认） | 普通库，运行期在场 | `[dependencies]` | 是 | `libs/` |
| `compile-time` | 只在编译器进程里活 | `[analyzers]` | **永不** | `compiler-libs/` |
| `contract` | 编译期扩展的 API 面 | 写扩展的工程的 `[dependencies]` | **永不**（编译器自带） | `compiler-libs/` |

对应改动面：

1. **解析域一分为二**。`[analyzers]` 与「role = compile-time / contract 的工程」的
   `[dependencies]` 从 `compiler-libs/` 解析；普通工程仍只看 `libs/`。
   `z42c.semantics` 进 `compiler-libs/`，**不进** stdlib workspace ⇒ 上面 2/3/4 一个都不触发。
2. **publisher 判据换成读 role**，不再靠「在不在 libs/」+「在不在 src/libraries/」两个代理。
   两条打补丁的注释随之退休。
3. **`role = compile-time` 的包永不进任何 payload 闭包** —— 这条今天靠「z42c 自建无
   `[analyzers]`」的巧合成立（`D9` 红线：不链入目标产物），改成由 role 直接保证。

## 参照系（他语言怎么切这一刀）

调研了 Rust / Swift / Java / .NET / Kotlin KSP / Dart / Scala 七家。**这一刀全都切了**，
而且切出来的东西高度一致：编译期扩展是**另一类 artifact**，有自己的 kind 或 scope 或搜索路径，
且**从不链入产物**。

| | 独立的**包 kind** | 独立的**依赖段/scope** | 独立的**解析路径** | 契约 API 由谁发 |
|---|---|---|---|---|
| Rust | ✅ crate type `proc-macro` | ❌ 普通 `[dependencies]` | ✅ host unit graph（与 target 分离） | **工具链**（sysroot 的 `proc_macro`，不在 crates.io） |
| Swift | ✅ `.macro` target | ❌（SE-0394 自己写明这是缺口） | ✅ 独立进程 + 版本化协议 | **普通包**（swift-syntax） |
| Java | ❌ | ✅ `annotationProcessorPaths` | ✅✅ **`-processorpath` 是一等的编译器 location** | **JDK**（`java.compiler` 模块，写 processor 零依赖） |
| .NET | ❌ | ✅ NuGet `analyzers` 资产组 + `PrivateAssets` | ✅ `analyzers/dotnet/cs/` | **普通 NuGet 包**（compile-only），实现在 SDK 里 |
| KSP | ❌ | ✅ `ksp(...)` 配置 | ✅ 独立 processor classpath | 普通 artifact |
| Dart | ❌ | ✅ `dev_dependencies`（不传递） | ✅ 两级 `cache`→`source` | 普通 pub 包 |
| Scala | ❌ | ✅ Ivy `plugin` 配置 | ❌ | 普通 artifact（编译器内部，无稳定 API） |

三条与本提案直接相关的结论：

1. ⚠️ **z42 现在踩的坑是有名字的已知失败模式。** 调研结论原文：
   > 「**如果你把契约 API 放进 SDK，它必须能被 SDK 常规的依赖解析路径解析到。**
   > 『在 SDK 目录里、但不在解析器会去看的地方』就是那个失败模式——插件作者**根本写不出插件**，
   > 而且没有任何东西会诊断它。」

   这正是 `z42c.semantics.zpkg` 今天的处境（在 `programs/z42c/`，不在 `libs/`），连
   「没有任何东西会诊断它」都对上了 —— 用户拿到的是位置在别处的 `E0443 undefined type`。

2. **「独立 kind」比「独立 scope」强，但两个都要。** Rust 有 kind 没 scope，于是表达不了
   「只要构建顺序不要链接」；Swift 有 kind 没 scope，SE-0394 明确写了这个遗憾；.NET 有 scope
   没 kind，于是「别把 Roslyn 打进包里」只能做成 **lint**（RS1038/RS1041）而不是清单事实。
   **z42 已经有了 scope（`[analyzers]` 就是 processorpath），缺的正是 kind。** 本提案补的就是它。

3. **契约由工具链发 = 最高杠杆的那个选择。** Java（`java.compiler`）与 Rust（sysroot `proc_macro`）
   让插件作者写**零依赖**，世上只存在一个版本的接口；Swift 走了反面（swift-syntax 当普通包），
   代价是全 workspace 被迫收敛到同一版本、编译分钟级、最后不得不做一套预编译二进制缓存打补丁。
   ⇒ z42 该走 Java/Rust 那条路：契约随 SDK 发，但**必须落在解析器看得见的地方**。

   附带好处：**版本偏斜 z42 已经解决了**。.NET 要靠 `CS9057`（analyzer 必须 ≤ 编译器）+ 多
   目标 `roslyn4.0/` 目录，Scala 要靠 `CrossVersion.full` 每个补丁版重发一遍 —— 而 z42 的
   **strict-pin 格式策略**（`zpkg minor N not supported`，无跨版本兼容）本来就强制
   「插件与编译器同代」。这条在别处是痛点的轴，在 z42 是既成事实，不额外欠债。

## 明确不改（Out of Scope）

- **不改包名 / 命名空间**（同 `converge-z42-syntax-lib` 的做法）。
- **不动 `z42c.core` / `z42c.syntax` 现在的位置** —— 它们确实有 runtime 消费方
  （scripting / playground / wasm），是真的 `contract` + `runtime` 双身份，单独判定。
- **不做「curated 子集」**：不试图从 `z42c.semantics` 里裁一个小契约包。`Generator` 契约暴露
  `Z42ClassType` / `SymbolTable` / `TypeSymbol`，与 Roslyn 的 generator 依赖完整语义 API 同形；
  裁子集等于重做一遍语义层的 API 分层，代价与收益不成比例。

## 待 User 裁决

1. **要不要引入 role 这个维度**（还是先接受「把 z42c.semantics 搬进 stdlib workspace」的副作用，
   把 role 留到以后）。
2. role 的取值与命名（`compile-time` / `contract` 两者要不要合一 —— 合一更简单，代价是
   「契约包」与「插件包」在 payload 判定上是同一条规则，目前看不出区别）。
3. 解析域目录名（`compiler-libs/` vs `libs/compile-time/` vs 别的）。
