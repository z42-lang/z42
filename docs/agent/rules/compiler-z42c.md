# z42c 自举编译器开发规范

> z42c **用 z42 语言自身写**（`src/compiler/z42c.*`，编译为 zpkg）。本文件只放**写 z42c 代码时的约定**；
> 子包结构、编译流程、「新增词法元素 / 语法 / IR 指令」的改动清单都在书里，入口见
> [开工导航](../README.md#开工先找-readme-和书)的「编译器」行。自举纪律（新语法 / 格式 support 先行、
> 晚一个 nightly 再 use）见 [bootstrap-seed.md](bootstrap-seed.md)。

## 代码约定

- **受限写法**：z42c 源码避开 z42 尚未完整支持的特性——无 `enum`（用 `static class` + `int` 常量替代，
  见 `TokenKind` / `DiagnosticCodes`）、泛型字段倾向用 typed array。沿用这套既有写法，**不要引入新依赖**。
- **Parser 不抛异常**：持 `DiagnosticBag`，遇错调 `_diags.Error(code, msg, span)` 累积后继续；错误码集中在
  `z42c.core/src/DiagnosticCodes.z42`；消息面向语言使用者（`expected '(' but got '{token}'`），
  不暴露内部枚举名。
- **局部变量不做块级 shadow**：嵌套块里的局部变量名不得与外层同名——编译不报错，静默复用外层那个
  （见 [common-pitfalls.md §3](common-pitfalls.md)）。
- 测试放 `src/compiler/<member>/tests/<name>/`；编译器正确性由 z42c 自举 stage 保证（`xtask test compiler`）。
