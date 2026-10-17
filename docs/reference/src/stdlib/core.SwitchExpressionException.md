<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SwitchExpressionException

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Exceptions/SwitchExpressionException.z42:7`

`switch` 表达式求值时没有任何臂匹配（含模式匹配了但守卫为假）。

只有表达式形态会抛：表达式必须产出一个值，无臂可采纳时无值可产。`switch` 语句不产值，无匹配
case 就什么都不做、不抛（C# 同）。抛出点的源码位置在栈回溯里，不在 Message 里。

```z42
public class SwitchExpressionException : Exception {
    SwitchExpressionException(string message)
}
```

### SwitchExpressionException(string message)

`src/libraries/z42.core/src/Exceptions/SwitchExpressionException.z42:9`

用消息构造。

