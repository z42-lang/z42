# z42c 自举编译器开发规范

> z42c **用 z42 语言自身写**，编译为 zpkg。本文件只放写 z42c 代码时的约定；编译器的机制与「新增词法 / 语法 /
> IR 指令」的改动清单在书里，入口见[开工导航](../README.md#开工先找-readme-和书)的「编译器」行。
> 自举纪律（新语法 / 格式 support 先行、晚一个 nightly 再 use）见 [bootstrap-seed.md](bootstrap-seed.md)。

## 代码约定

- **只用当前语言子集**：遇到表达不了的写法，停下汇报——能补进语言的当次补，**禁止在编译器代码里写 workaround**。
  现有的受限写法见[编译器自举 · 受限写法约定](../../internals/src/compiler/self-hosting.md)，沿用它们；不引入新依赖。
- **编译器零 `extern` / `[Native]`**：`src/compiler/**` 不直接绑 VM builtin，需要的能力调 `z42.core`（或运行时非必需
  的独立库）的公开 API；缺 API 就先在 core 补通用原语再调用。规则见
  [包划分 · extern 落点](../../internals/src/stdlib/organization.md#extern-落点只有两处)。
- **Parser 遇错不抛异常**：记录诊断后继续解析。错误码集中登记（新增流程见
  [错误码体系](../../internals/src/compiler/error-codes.md)）；消息面向语言使用者，不暴露内部名字。
