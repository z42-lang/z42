<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.INumber

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Protocols/INumber.z42:12`

数值接口（对齐 C# 11 static abstract）：声明 5 个二元算术运算符为 `static abstract`。

实现者（基础类型结构体、用户结构体/类）须提供 `static override` 对应。泛型代码
`where T : INumber` 里写 `a + b`（或 mangle 形式 `T.op_Add(a, b)`），编译器经值驱动的
VCall 派发到具体实现者 —— 接收者的运行时类决定跑哪个 `op_Add`。

比较走 IComparable；相等走 IEquatable。
不支持混合类型算术（仅 Self + Self → Self）—— 正因如此，原先的型参 `T` 不携带任何
信息（`where T : INumber<T>` 纯是「我指我自己」的样板），已由 `Self` 取代。

```z42
public interface INumber {
    static abstract Self op_Add(Self a, Self b)
    static abstract Self op_Subtract(Self a, Self b)
    static abstract Self op_Multiply(Self a, Self b)
    static abstract Self op_Divide(Self a, Self b)
    static abstract Self op_Modulo(Self a, Self b)
}
```

### static abstract Self op_Add(Self a, Self b)

`src/libraries/z42.core/src/Protocols/INumber.z42:14`

加法。

### static abstract Self op_Subtract(Self a, Self b)

`src/libraries/z42.core/src/Protocols/INumber.z42:16`

减法。

### static abstract Self op_Multiply(Self a, Self b)

`src/libraries/z42.core/src/Protocols/INumber.z42:18`

乘法。

### static abstract Self op_Divide(Self a, Self b)

`src/libraries/z42.core/src/Protocols/INumber.z42:20`

除法。

### static abstract Self op_Modulo(Self a, Self b)

`src/libraries/z42.core/src/Protocols/INumber.z42:22`

取模。

