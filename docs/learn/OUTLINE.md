# 《z42 学习手册》目录规划

> 活的写作计划（`src/SUMMARY.md` 只收已写完的章）。写作规范见 [learn-writing.md](../agent/rules/learn-writing.md)。
> 每章一行：页面路径（= `src/<路径>.md` = `examples/<路径>/`）｜要点｜状态。
> 状态：🟢 已写 ｜ ✅ 可写 ｜ 🧪 动笔前需核实特性完成度 ｜ ⏸ 暂缓（特性未落地）

## 第一部分 · 起步（getting-started）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 1 | 安装 z42 | `getting-started/install` | 平台支持表（无 Intel Mac）、一行命令安装、PATH、`z42 --version` 验证、安装选项、更新与卸载 | 🟢 |
| 2 | Hello, World | `getting-started/hello-world` | 写 `hello.z42`（5 行，无 namespace / 无清单）→ `z42 run hello.z42` → 读懂代码 → 命令行参数 → 编译错误 | 🟢 |
| 3 | 工程与构建 | `getting-started/projects` | 什么时候需要工程 → `z42 new` → `z42.toml` 各字段 → 多源文件与 `namespace` → `z42 build` / `--release` / 产物 → `z42 clean` | 🟢 |
| 4 | 开发环境 | `getting-started/tooling` | `z42d install vscode`、`z42 repl` + 元指令、读懂编译错误（错误码链接参考手册） | 🟢 |

## 第二部分 · 语言基础（basics）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 5 | 变量与基本类型 | `basics/variables` | `var`、整数/浮点/`bool`/`char`/`string`、字面量、类型转换（比 C# 严）、`const`/`readonly`、`?` 只是标注 | 🟢 |
| 6 | 运算符与表达式 | `basics/operators` | 算术 / 比较 / 逻辑 / 位运算、可空类型、`?.` / `??` | 🟢 |
| 7 | 控制流 | `basics/control-flow` | `if` / `switch` / `while` / `do` / `for` / `foreach` / `break` / `continue` | ✅ |
| 8 | 函数 | `basics/functions` | 自由函数与方法、默认值、命名实参、`params`、`ref` / `out` / `in`、局部函数、递归 | ✅ |
| 9 | 字符串 | `basics/strings` | 插值、原始字符串 `"""`、常用成员、格式化 | 🧪 |
| 10 | 数组与集合 | `basics/collections` | 数组、`List` / `Dictionary`、集合字面量与展开、遍历 | ✅ |
| 11 | 元组 | `basics/tuples` | 值元组、解构 | ✅ |

## 第三部分 · 类型与抽象（types）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 12 | 类与对象 | `types/classes` | 字段、构造器与 `: this()`、属性、索引器、对象初始化器、target-typed `new`、`static` | ✅ |
| 13 | 继承与多态 | `types/inheritance` | `virtual` / `override` / `abstract` / `sealed`、`base`、构造链 | ✅ |
| 14 | 接口 | `types/interfaces` | 定义与实现、多接口、接口属性 / 索引器 | ✅ |
| 15 | 值类型与记录 | `types/structs-records` | `struct` 值语义、`[Record]` 与主构造器、`with` 表达式 | 🧪 |
| 16 | 枚举与模式匹配 | `types/patterns` | `enum`、`switch` 表达式、结构化模式、解构、穷尽性 | 🧪 |
| 17 | 泛型 | `types/generics` | 泛型类型、泛型方法、`where` 约束 | ✅ |
| 18 | Lambda、闭包与委托 | `types/lambdas` | lambda 语法、捕获语义、委托、`methodof` | ✅ |
| 19 | 异常处理 | `types/exceptions` | `try` / `catch` / `finally` / `throw`、自定义异常 | ✅ |
| 20 | 组织代码 | `types/organization` | `namespace`、`using` / `global using` / 类型别名、访问控制、`partial`（`global using` 跨文件目前没有端到端测试，本章示例将是首个覆盖） | ✅ |
| 21 | 特性与反射入门 | `types/attributes-reflection` | 使用内置 attribute、`typeof` / `GetType`、成员查询 | 🧪 |

## 第四部分 · 标准库实战（stdlib）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 22 | 文件与目录 | `stdlib/files` | `File` / `Directory` / `Path`、读写文本与字节 | ✅ |
| 23 | 数据格式 | `stdlib/data-formats` | JSON serde、TOML、YAML | ✅ |
| 24 | 文本处理 | `stdlib/text` | `StringBuilder`、正则 | ✅ |
| 25 | 命令行程序 | `stdlib/cli` | 读取参数、`Std.Cli` 参数解析、退出码、运行外部进程 | ✅ |
| 26 | 线程与并发 | `stdlib/concurrency` | `Thread`、`Channel`、同步原语 | 🧪 |
| 27 | 网络 | `stdlib/networking` | HTTP 客户端 | 🧪（transcript 不能依赖外网，需本地服务端方案） |
| — | async / await | — | 词法已有关键字，无测试覆盖 | ⏸ |

## 第五部分 · 工程化（engineering）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 28 | 编写测试 | `engineering/testing` | 工程的 `tests/` 目录（自动发现）、`[Test]` / `Assert`、`[Setup]` / `[ShouldThrow]`、`z42 test --filter` | ✅ |
| 29 | 依赖与工作区 | `engineering/workspaces` | 库工程、依赖声明、`z42.workspace.toml`、多成员构建 | 🧪 |
| 30 | 运行时配置 | `engineering/runtime-config` | `--mode interp/jit`、`--set`、配置文件、`AppProperties` | ✅ |
| 31 | 发布应用 | `engineering/publishing` | `z42 publish`、self-contained、目标平台 rid | 🧪 |

## 第六部分 · 跨平台与嵌入（platforms）

| # | 章节 | 路径 | 要点 | 状态 |
|---|------|------|------|------|
| 32 | WebAssembly | `platforms/wasm` | wasm workload、在网页中运行 | 🧪 |
| 33 | iOS 与 Android | `platforms/mobile` | workload 安装、`z42 export` 生成原生工程 | 🧪 |
| 34 | 在 C / Rust 程序中嵌入 z42 | `platforms/embedding` | C ABI、全平台同一份 `main.c`、Rust 宿主 | 🧪（会话脚本需支持外部工具 `cc` / `cargo`） |

## 附录（appendix）

| 章节 | 路径 | 说明 |
|------|------|------|
| CLI 速查 | `appendix/cli` | 从 `z42 --help` 输出生成的 transcript |
| 写给 C# 开发者 | `appendix/from-csharp` | 相同 / 不同之处对照 |
| 常见编译错误 | `appendix/errors` | 高频错误码 + 触发示例（transcript 带 `[exit: 1]`），深入解释链接知识库错误码页 |

## 写作批次

| 批次 | 内容 | 状态 |
|------|------|------|
| 0 | 手册骨架 + 示例门禁 + 第 1、2 章 | 🟢 |
| 1 | 第 3、4 章（第 3 章的单文件运行依赖 `z42 run hello.z42`） | 🟢 |
| 2 | 第二部分 | |
| 3 | 第三部分 | |
| 4 | 第四、五部分（🧪 章节先核实） | |
| 5 | 第六部分 + 附录（嵌入章节需要会话脚本支持外部工具） | |
