# 第一部分 · 语言（Language）

z42 的语法、类型系统、内存模型、内置协议与 FFI 表面——即"作为一门语言，z42 是什么样"。

## 章节规划

| 章节 | 涵盖 |
|------|------|
| 语法与词法 | 词法、语句、表达式、字符串字面量、语法配置 |
| 类型系统 | 基本类型、泛型、装箱、静态抽象接口 |
| 所有权与内存模型 | 值/引用语义、所有权 |
| 内置协议 | object protocol、定制点、迭代协议、属性 |
| 异常与错误处理 | 异常语义 |
| 命名空间与访问控制 | namespace/using、访问修饰 |
| FFI / interop 表面 | 与 native 的语言级接口、反射、元编程 |

## 迁移状态（旧 `docs/design/language/` → 本部分）

> ⬜ 待迁 · 🟡 迁移中 · ✅ 已迁并校对。全部 ✅ 后删除旧目录。

| 旧文档 | 目标章节 | 状态 |
|--------|---------|------|
| language-overview.md | （贯穿全部；参考手册基线） | ⬜ |
| syntax-config.md / raw-string-literal.md / string-builtins.md / compound-assign.md | 语法与词法 | ⬜ |
| ~~generics.md~~ ✅ / boxing.md / static-abstract-interface.md | 类型系统 | 🟡 部分（generics.md 已于 2026-09-08 迁入 [generics.md](generics.md)） |
| object-protocol.md / customization.md / iteration.md / properties.md | 内置协议 | ⬜ |
| exceptions.md | 异常与错误处理 | ⬜ |
| namespace-using.md / access-control.md / naming-conventions.md | 命名空间与访问控制 | ⬜ |
| interop.md / reflection.md / metaprogramming.md / attributes.md | FFI / interop 表面 | ⬜ |
| arrays.md / foreach.md / closure.md / delegates-events.md / parameter-modifiers.md | 语法与词法 / 类型系统（按特性归位） | ⬜ |
