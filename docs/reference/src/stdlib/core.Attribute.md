<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Attribute

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Attribute.z42:10`

所有用户自定义 attribute 的基类。

用户 attribute 是派生自 `Attribute` 的普通类，类名必须以 `Attribute` 后缀结尾
（E0444 强制）：`class RouteAttribute : Attribute`，使用时写去后缀的 `[Route]`。
经反射读回——`Type.GetCustomAttributes()` / `MethodInfo.GetCustomAttributes()` 返回活实例
（重复调用同一缓存实例）。对标 C# `System.Attribute`，但 z42 把后缀设为强制（硬错误，
而非 C# 的可选 CA1710 警告）。

```z42
public class Attribute {
}
```

