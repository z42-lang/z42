<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.IFormattable

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Protocols/IFormattable.z42:11`

自定义格式化契约：用格式串生成字符串表示。

与 Object.ToString()（无参）通过 arity 重载共存：
  - obj.ToString()    —— Object 协议（默认表示）
  - obj.ToString(fmt) —— IFormattable 协议（带格式参数）

format 字符串的语义由实现者决定（无统一格式语法）。例如数值类型可能识别
"X8" 表示 8 位 hex；日期类型可能识别 "yyyy-MM-dd" 等。

```z42
public interface IFormattable {
    string ToString(string format)
}
```

### string ToString(string format)

`src/libraries/z42.core/src/Protocols/IFormattable.z42:13`

按 format 格式串返回字符串表示。

