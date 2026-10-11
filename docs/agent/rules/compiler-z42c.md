# z42c 自举编译器开发规范

> z42 的编译器 z42c **用 z42 语言自身写**（`src/compiler/z42c.*`，编译为 zpkg）。
> 本文件是**写 z42c 代码时的行为约束**；子包结构与 Lexer / Parser / AST 的机制见
> [架构总览](../../internals/src/compiler/architecture.md) 与
> [源代码编译流程](../../internals/src/compiler/source-compile.md)。
>
> **要新增词法元素 / 语法 / IR 指令**（改哪些文件、按什么顺序、先评估哪些更便宜的路）见
> [扩展语言与 IR：新增词法元素 / 语法 / IR 指令](../../internals/src/compiler/extending-language.md)。
> Rust VM 侧见 [runtime-rust.md](runtime-rust.md)。

> **受限写法**：z42c 源码避开 z42 尚未完整支持的特性——无 `enum`（用 `static class` + `int` 常量替代，
> 见 `TokenKind` / `DiagnosticCodes`）、泛型字段倾向用 typed array。写编译器代码时沿用这套既有写法，
> **不要引入新依赖**。

> **⚠️ 局部变量不做块级 shadow**（写任何 z42 代码都适用，见 [common-pitfalls.md §3](common-pitfalls.md)）：
> 嵌套块里的局部变量名不得与外层已声明的同名——编译不报错，静默复用 / 覆盖外层那个。

## 代码约定

- **Parser 不抛异常**：持 `DiagnosticBag`，遇错调 `_diags.Error(code, msg, span)` 累积后继续；错误码集中在
  `z42c.core/src/DiagnosticCodes.z42`；消息面向语言使用者（`expected '(' but got '{token}'`），
  不暴露内部枚举名。
- **改某块代码前先读该子包的 `README.md`**（子包划分见
  [架构总览](../../internals/src/compiler/architecture.md)）。
- 测试放 `src/compiler/<member>/tests/<name>/`；编译器正确性由 z42c 自举 stage 保证（`xtask test compiler`）。

## 分阶段引入语法 / 格式（自举纪律）

z42c 自身用 z42 写、由**上一个已发布 nightly 的 z42c** 编译，因此**新语法 / 新 zbc·zpkg 格式必须
support 先行、晚一个 nightly 再 use**——否则跨版本自举断链。完整纪律 + `xtask test compiler bootstrap`
边界检查见 [bootstrap-seed.md](bootstrap-seed.md)。

## 资源加载顺序

加载 zpkg / module / 注册 builtin 的循环（`read_dir` / 容器迭代 + first-wins 写入）必须先按稳定键排序，
禁止依赖文件系统 / hash 的"碰巧顺序"。该约束跨语言适用，统一沉淀在
[common-pitfalls.md §1](common-pitfalls.md#1-资源加载顺序必须显式排序)。
