<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.MissingSymbolException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/MissingSymbolException.z42:8`

运行时解析不到某个符号（函数 / 类型 / 字段 / 构造器）——通常意味着依赖版本 skew： 编译时依赖新版，运行时加载到的却是缺该符号的旧版。

想在缺符号时主动降级而不是收异常，用编译期宏 `available!(X)` 把那条分支保护起来——被保护的分支
会在加载期整块剪掉，其符号永不参与解析，因此不会触发本异常。

```z42
public class MissingSymbolException : Exception {
    MissingSymbolException(string message)
}
```

### MissingSymbolException(string message)

`src/libraries/z42.core/src/Exceptions/MissingSymbolException.z42:10`

用消息构造。

