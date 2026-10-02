# overload_class_hierarchy_cross_pkg

`fix-overload-ref-conversion` 的**跨包**守卫。

重载集在 `main`，类层次（`Leaf : Mid : Base`、`Leaf : IMark`）在 `target`。
消费方判定适用性时必须沿 **TSIG 重建的** 基类名与接口表走链 ——
`ClassExtractor` 写 `baseName` / `Interfaces`（生产侧已展开为传递闭包），
`ImportedSymbolLoader` 读回填进 `Z42ClassType`。

覆盖：跨包三层继承 / 跨包择优（更派生者胜）/ 跨包类→接口 / 中间层 / 基类本身。

⚠️ 已知隐患（本用例正是为它设的网）：`SymbolTable.IsSubclassOf` 靠 `GetClass(cur)`
逐级取，链中某一级类没被载进 `r.Classes` 时会返 null 断链 → 判不适用。
三层继承比两层更容易暴露这个问题。
