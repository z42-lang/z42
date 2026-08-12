# Proposal: 默认成员可见性 = private（对齐访问控制规范）

## Why

规范冲突（2026-08-12 发现）：语言规范 [`docs/design/language/access-control.md`](../../../design/language/access-control.md)
规定「默认可见性 = 最小封闭作用域」——**类成员无修饰符默认 `private`**（封闭层=类），顶层类型/自由函数默认
`internal`（封闭层=模块）。但实现（`SymbolCollector._vis` + enforce-access-control #180）把**所有**无修饰符
默认为 `internal`，与规范不符：`class A { int a; } class B { …x.a }` 实现放行、规范应拒。

User 裁决 **以规范为准（方案 B）**：改实现，无修饰符成员 = `private`。顺带补齐规范 Phase-1 明列却未实现的
**组合修饰符拒绝**。

## What Changes

- **无修饰符成员默认 `private`**（字段/方法/构造器/属性/索引器）；无修饰符**顶层/自由函数**保持 `internal`。
  `_vis` / `_visCode` 无修饰符默认改为**按位置传入**（`_methodSymbol` 以 `containing==""` 区分自由函数）。
- **组合访问修饰符 → E0405**（`protected internal` / `private protected` 等 2+ 访问修饰符），`_parseModifiers` 拦截。
- 破坏面修正（无修饰符 → 显式 `internal`，同包协作的正确标注）：stdlib 4 处（BigInt×2/Blake3/Sha256）+
  xtask 脚本 4 处（MicroBenchAgg）。测试/e2e fixture 补显式修饰符（详见 tasks）。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `_vis(mods, dflt)` 加位置默认；成员传 "private"、自由函数传 "internal" |
| `src/compiler/z42c.semantics/src/IrGenFacts.z42` | MODIFY | `_visCode(mods, dflt)` 加位置默认 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 成员 `_visCode(...,1)`、自由函数 `_visCode(...,3)` |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 字段 `_visCode(...,1)` |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | `_parseModifiers` 组合访问修饰符 → E0405 |
| `src/libraries/z42.numerics/src/BigInt.z42` | MODIFY | `_fromMagSign`/`_oneMag` → internal |
| `src/libraries/z42.crypto/src/{Blake3,Sha256}.z42` | MODIFY | 1 helper each → internal |
| `scripts/xtask_bench.z42` | MODIFY | MicroBenchAgg 4 helper → internal |
| `scripts/build/xtask_compiler_e2e.z42` | MODIFY | e2e fixture `Counter.Bump`/`Greeter.Hello` → public |
| `src/compiler/z42c.semantics/tests/typecheck/typecheck_tests.z42` | MODIFY | 5 fixture 补显式修饰符 |
| `src/compiler/z42c.semantics/tests/access-control/access_control_tests.z42` | MODIFY | 重定默认-private + 加组合修饰符测试 |
| `docs/design/language/access-control.md` | MODIFY | Status 更新（全部实现） |
| `docs/book/src/compiler/access-control.md` | MODIFY | 默认可见性节 = 最小封闭作用域 + 组合拒绝 |

## Out of Scope

- **类级访问强制**（private 嵌套类 / internal 类不可跨作用域*引用*）—— 当前只强制成员访问；独立后续 change。
- 枚举成员带修饰符的诊断消息优化（现已拒绝，消息较杂）。

## Open Questions

- 无。方案 B 已裁决。
