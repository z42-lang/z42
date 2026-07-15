# Tasks: 跨包静态调用按命名空间消歧

> 状态：🟢 已完成 | 完成：2026-07-16 | 类型：fix（compiler）

- [x] 1. `DependencyIndex`：`AddModule` 注册全名键 + `GetStaticScoped`（恰好一个不同实体命中）
- [x] 2. `EmitContext`/`IrGen` 加 `ActiveNs`；`FunctionEmitter` 3 处 copy
- [x] 3. `IrDump._activeNamespaces`（usings + 本 ns）+ 两编译入口设 `gen.ActiveNs`
- [x] 4. `ExprEmitter` 静态调用：scoped 优先 → 短键回落；local-wins 守卫
- [x] 5. 单测 `depindex_tests.z42::test_scoped_disambiguates_cross_ns_same_shortname`
- [x] 6. 自举不动点 7/7 byte-identical（含守卫）
- [x] 7. 共存实测：登记 z42.project 编 z42c 不崩 + with/without z42.project 7/7 逐字节相同（已回退登记）
- [x] 8. `common-pitfalls.md §1` 注记根治
- [ ] 9. 全 gate GREEN 以 CI 为权威（push 后盯）

## 备注

- VM 侧零改动（实测：VM 按全 FQN 精确匹配，不同 ns 天然不撞键；问题纯编译期）。
- 使能 `converge-z42c-onto-z42-project`（path C，User 裁决）：converge 现可原子落地，无需 rename-前置 / CI 手术。
