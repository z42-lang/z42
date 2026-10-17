<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.ForwardAttribute

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/ForwardAttribute.z42:11`

`[Forward]` —— 成员转发：贴在**字段**上，让外层类把该字段的部分成员面提升上来， 用组合替代 z42 不支持的多继承。

生成的是**真方法**（不是名字解析改写）——只有真方法才能满足接口、被反射看见。
三种用法由实参形状选档：`[Forward]`（配 `partial` 声明，用户写签名）、
`[Forward(typeof(ILogger))]`（接口面，签名由接口钉死）、
`[Forward(methodof(Logger.Log(string)))]`（逐个点名）。`ToString` / `Equals` /
`GetHashCode` / `GetType` 永不转发（转发它们会让外层对象谎报身份）；不传递、不递归。

```z42
public class ForwardAttribute : Attribute {
    object Target
    ForwardAttribute()
    ForwardAttribute(object target)
}
```

### object Target

`src/libraries/z42.core/src/ForwardAttribute.z42:13`

转发目标（`Type` / `MethodInfo` / null，按档位）。

### ForwardAttribute()

`src/libraries/z42.core/src/ForwardAttribute.z42:15`

无参构造（配 `partial` 声明档）。

### ForwardAttribute(object target)

`src/libraries/z42.core/src/ForwardAttribute.z42:17`

带目标构造（接口面 / 逐个点名档）。

