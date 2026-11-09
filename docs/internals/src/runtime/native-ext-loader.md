# Compression 原生模块

> 代码：`src/runtime/crates/z42-compression/`、`src/runtime/src/loader/native.rs`、`src/runtime/src/interop/native_call.rs`

## 模块边界

`z42_compression` crate 同时拥有压缩算法、独立 C API 和脚本接口描述符。`registration.rs` 导出 `z42_compression_module_v1`，描述 DEFLATE、Zstandard、Brotli、LZ4 及流式 begin/feed/finish/dispose 的参数、返回值和通用调用入口。运行时只装配描述符，不复制这份函数清单。

crate 的 `backend` feature 启用算法实现；关闭时仅保留接口声明，没有可调用 wrapper 或算法状态。interop 首次调用时通过 loader 解析完整模块入口，核对名称、全部签名与 ABI，成功后发布每 VM 的实现并直接调用。声明和实现来自同一份签名清单。流式压缩句柄仍归算法实现管理。

## 加载与注册

host 默认注册模块描述符，查询本身不访问文件系统。首次调用声明中的函数时，loader 先查宿主注册的静态符号解析器；没有静态提供方时才按逻辑库名查询 host 注入的候选路径，并经 PAL 装入动态库。动态库代码由 PAL 保留到进程退出；每 VM 缓存命名解析结果。没有使用压缩功能时不加载完整算法镜像。

静态宿主可直接链接启用 `backend` 的实现，或注册静态库解析器供 loader 查询。在禁止动态库的平台上，这条路径仍可工作。各平台具体归档和动态库发布、默认 WASM 包含项由[组件化运行时](componentized-runtime.md)定义。

## 值与错误

通用 ABI 以指针/长度借用字节数据，由接收方复制。算法的 Rust 容器不会穿过 C 调用；运行时只在调用结束后分配托管返回值。压缩错误通过通用异常报告入口复制类名和消息；独立的传统 `z42_compression_*` C API 继续保留返回码、线程 last-error 和分配侧 free 规则。

WASM 上 Zstandard 仍返回既有的不支持错误；其 C 依赖尚未纳入该目标，不因注册方式改变而宣称支持。

## 验证

在 `src/runtime/` 运行 `cargo test -p z42-compression` 和 `cargo test -p z42 --lib interop::`；仓库根运行 `xtask test stdlib z42.compression`、完整 `xtask test` 及 `xtask package sdk --verify`。精简 WASM 构建验证无需 libffi 的静态调用路径。

## Deferred

### Migration of existing stdlib natives

其他标准库原生实现仍使用 corelib 内部 builtin 协议，不要求为这轮注册机制全部迁移。来源是模块独立化设计；前提是出现需要独立交付或隔离依赖的模块；触发条件是该模块开始拆分；当前直接注册其内部 `BuiltinDesc`。独立模块应采用通用 C 描述符，避免新增运行时专用 wrapper 清单。
