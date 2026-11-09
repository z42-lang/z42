# 嵌入

在 C / Rust 宿主程序里嵌入 z42 运行时：C ABI 契约、生命周期、数据编组。

> 这里讲**契约**（你的宿主代码该怎么调）。VM 内部如何实现这套 ABI，见实现内幕。

## 各页

- [C ABI 契约](c-abi.md)
- [原生互操作（native interop）](native-interop.md)
- [Rust VM 扩展状态](vm-extensions.md)

Rust 嵌入统一从 z42::host 导入接口与类型。LoadOptions.backends 可分别为 interp/JIT 选择 Auto（优先静态，再查动态库）、Linked、Dynamic(path) 或 Disabled；执行模式只选择可用后端。请求未链接、缺失、禁用或平台不支持的后端会返回错误。JIT 模式同时需要基线解释器。
