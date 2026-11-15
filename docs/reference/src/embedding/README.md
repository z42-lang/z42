# 嵌入

在 C / Rust 宿主程序里嵌入 z42 运行时：C ABI 契约、生命周期、数据编组。

> 这里讲**契约**（你的宿主代码该怎么调）。VM 内部如何实现这套 ABI，见实现内幕。

## 各页

- [C ABI 契约](c-abi.md)
- [原生互操作（native interop）](native-interop.md)
- [Rust VM 扩展状态](vm-extensions.md)

Rust 嵌入使用 z42::host 的加载与执行接口。LoadOptions.mode 请求执行模式，native 设置原生代码搜索目录和三种来源许可，allow_profiling 控制可选诊断能力。具体组件装配保持内部；显式 JIT 请求不可用时返回错误，不静默降级。
