# crosspkg_record_members — 跨包 `[Record]` 类的成员可用性

**这道门盯的是：导入的 record 必须**仍然是** record。**

`target` 导出 `[Record] public class Pt(int X, int Y)` 与一个普通 `class Plain`（对照）；
`main` 对 `Pt` 用 `with` 表达式与位置解构模式 `o is Pt(a, b)`。

## 修前行为（实测，非推断）

两者**编译期误拒**：

```
E0402: `with` requires a record type: Pt
E0402: positional pattern requires a record type: Pt
```

根因：`Z42ClassType.IsRecord` 的唯一写入点是 `StubCollector._putClassStub`（**本地**类），
`ImportedSymbolLoader` 从不设置 ⇒ 导入 record 恒 `IsRecord == false`，而
`ConstructTyper._bindWith` 与 `PatternBinder._bindPositional` 正以 `!IsRecord` 为拒绝条件。
`CLASS_FLAG_RECORD`（TYPE Flags bit3）本就在 zbc 里，只是 `TsigReconcile` 从不读、
`ExportedClassZ` 也无处承载。

## 为什么 `Plain` 也在 fixture 里

阴性对照。没有它，「fixture 本身接线坏了」与「record 被误拒」两种失败长得一模一样——
我第一次做这个探针时正是先撞上前者（手工 `z42c build` 解析不到依赖），
靠加入 `Plain` 才分辨出来。
