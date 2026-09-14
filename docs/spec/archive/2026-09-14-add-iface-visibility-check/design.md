# 设计：接口成员可见性校验

## 落点

`src/compiler/z42c.semantics/src/InheritanceResolver.z42` `_checkOneIfaceMethod`。

MangleKey 命中后的校验链，插在 static 之后、返回之前（种类→可见性→返回，逐条独立发 E0412 后 return）：

```
if (it.IsImported) { return; }              // 既有：跨包早退
if (cm.IsStatic != ims.IsStatic) { … }      // 既有：种类
if (cm.Visibility != "public") { … }        // ★ 本 change：可见性
… 返回类型协变校验 …                          // 既有
```

判据一个条件即可：`cm.Visibility != "public"`。取值口径 `"public"`/`"private"`/`"internal"`/
`"protected"`（`SymbolCollector._vis`）。类成员无修饰默认 `"private"`（`SymbolCollector:385`），
故无修饰接口实现同样落进拦截——这是严格口径的直接后果，非额外分支。

## 为什么只覆本包

`it.IsImported` 早退在校验链最前（#636 建立）：导入接口成员的 `Visibility` 从导出/wire 侧
过来不可靠（同 `IsStatic`）。跨包漏报与 func-type/assoc-type 跨包漏报同取舍。真静默洞在本包。

## 零字节漂移论证

- 检查只产诊断、绝不回灌 `_withDefaults` 或发射端。
- 全仓无非-public 接口实现（`build stdlib` 25/25 + `build compiler` 自建 + `build test` 316 ok
  全量 `rm .cache` 重建，E0412 命中 = 0）。⇒ 正确路径零触发 ⇒ 发射零改动。
- 无格式 bump：复用 E0412、无新 IR/wire。

## 测试

`src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42` 补 5 条门：
- 4 负例：`private`/`internal`/`protected`/无修饰(默认 private) → E0412。
- 1 无误报守卫：显式 `public` → 不报。

退回对照（`if (false && …)` 禁用检查 + 重建）：4 负例恰 FAIL、public 守卫 + 既有 static 门
仍 PASS ⇒ 门精确钉在可见性检查。
