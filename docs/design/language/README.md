# design/language/

z42 语言设计文档：语法、语义、类型系统、内置协议与 FFI 表面。

## 职责

- 描述 z42 **作为语言**的全部对外行为（语法 / 语义 / 类型规则 / 内置类型）
- 不涉及编译器内部数据结构（归 [`../compiler/`](../compiler/)）或 VM 执行细节（归 [`../runtime/`](../runtime/)）

## 核心文件

### 参考手册（模板 C）

| 文件 | 内容 |
|------|------|
| [`language-overview.md`](language-overview.md) | 语言概览：所有语法 / 类型 / 控制流的用户视角描述，详细规则链接到下方文档 |
| [`grammar.peg`](grammar.peg) | PEG 语法规范（机器可读，SoT）|

### 类型系统与高级特性（模板 A）

| 文件 | 内容 |
|------|------|
| ~~`generics.md`~~ → [book/src/language/generics.md](../../book/src/language/generics.md) | 泛型 + where 约束 + 关联类型 + 具化策略（**2026-09-08 已迁入 book**，change `add-generic-type-arg-inference`） |
| [`closure.md`](closure.md) | 闭包：捕获语义 / 三档实现策略 / 单目标 |
| [`delegates-events.md`](delegates-events.md) | delegate 类型 + multicast + event 关键字 + ISubscription wrapper |
| [`static-abstract-interface.md`](static-abstract-interface.md) | C# 11 风静态抽象接口成员（INumber<T> 等）|
| [`interop.md`](interop.md) | 三层 ABI（C / Rust / 平台 facade）+ pinned/import 语法 |
| [`metaprogramming.md`](metaprogramming.md) | 编译期代码生成/宏（L3+ 底稿）：同语言 · 类型化 AST · quote/splice · 分层 derive→模板→变换 |

### 单一特性 / 实现规范（模板 B）

| 文件 | 内容 |
|------|------|
| [`parameter-modifiers.md`](parameter-modifiers.md) | `ref` / `out` / `in` 参数修饰符 |
| [`properties.md`](properties.md) | auto-property 语法与 desugar |
| [`arrays.md`](arrays.md) | 一维数组 + Std.Array 基类 |
| [`foreach.md`](foreach.md) | foreach 数组迭代 |
| [`iteration.md`](iteration.md) | 索引鸭子协议 + IEnumerable / IEnumerator 接口契约 |
| [`exceptions.md`](exceptions.md) | try / catch / throw + 标准异常类层次 + catch 类型过滤 |
| [`compound-assign.md`](compound-assign.md) | `+= / -= / *= / /= / %=` 复合赋值 |
| [`object-protocol.md`](object-protocol.md) | ToString / Equals / GetHashCode / GetType 派发协议 |
| [`string-builtins.md`](string-builtins.md) | string 内置方法 |
| [`namespace-using.md`](namespace-using.md) | namespace + using 导入 + strict resolution |
| [`access-control.md`](access-control.md) | public / internal / protected / private |
| [`customization.md`](customization.md) | LanguageFeatures + ParseTable 定制机制 |
| [`syntax-config.md`](syntax-config.md) | 三层语法配置 |

## 入口点

新读者建议入口：[`language-overview.md`](language-overview.md) → 按主题深入到具体文档。

## 依赖关系

- 上游：[`../philosophy.md`](../philosophy.md)（设计准则）、[`../../features.md`](../../features.md)（决策清单）
- 下游：[`../compiler/`](../compiler/)（这些语言特性的编译器实现）、[`../runtime/`](../runtime/)（VM 执行）
