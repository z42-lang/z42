# 原生模块注册与调用

> 代码：`src/runtime/src/interop/`、`src/runtime/src/loader/native.rs`、`src/runtime/crates/z42-abi/src/native.rs`、`src/runtime/crates/z42-rs/src/native.rs`

## 职责与装配

interop 校验、注册和查询 native 描述符，并负责脚本值与 C 值之间的转换；loader 定位并装入代码、保留库寿命；PAL 实现动态库 OS 原语。corelib 只实现核心脚本 API。Repl 和 compression 的脚本接口描述、调用入口与资源状态由各自 crate 拥有，host 选择默认注册集合。

| 提供方 | 实现 | 注册方式 |
|---|---|---|
| `z42.core` | `src/corelib/` | `NativeProvider` 提交内部 `BuiltinDesc` |
| `z42_repl` | `crates/z42-repl/src/native.rs` | `z42_repl_module_v1` 返回 C 描述符 |
| `z42_compression` | `crates/z42-compression/src/native.rs` | `z42_compression_module_v1` 返回 C 描述符 |

`RegistryBuilder` 在发布前校验整个提供方，拒绝空名、重复提供方和同一提供方内重复函数。`freeze` 产生不可变快照，同时生成内部 builtin 执行表及 metadata 名称/ID 目录。不同提供方可以导出同名函数，裸名歧义不会按注册顺序选择胜者。BuiltinId 只在当前执行实例内有效，字节码仍保存名称。

内部 builtin 保留按 ID 直接调用寄存器入口的路径。独立 native 模块走通用调用桥，不在 corelib 或 interop 中维护模块专用 wrapper 清单。默认描述符跨 VM 共享；模块实例及其 opaque 状态按 VM 创建、由模块自己的 destroy 回收，worker 共享所属 VM 的实例。构造器在注册表锁外执行；并发构造的多余实例会释放。

## 同一入口：静态注册与动态加载

`z42_native.h` 与 `z42_abi::native` 定义相同的 C 布局。模块入口返回 `Module`：ABI 版本/大小、逻辑名称、函数签名与调用指针，以及成对的可选状态构造/析构入口。静态链接直接调用该入口并提交描述符；`loader::native::load_native_module` 从动态库解析 `<basename>_module_v1`，再提交给同一个 `register_native_module`。校验失败不发布任何导出；同一 VM 的相同模块接口重复注册幂等，冲突接口拒绝替换。

宿主也可先用 `host::register_static_native_library` 或 C 的 `z42_host_register_static_native_library` 注册逻辑库名与 C 符号解析器。解析器和返回入口必须保留到进程退出。loader 优先查询该解析器，再检查 PAL 动态加载能力；已注册静态库缺失的符号直接报错，避免意外混用另一个动态镜像。解析器调用不持有注册表锁。

轻量 compression 注册代码在首次调用时通过宿主解析同一模块的完整实现入口；启用算法后端的静态构建直接调用自身实现。Repl 的轻量注册代码提供通用输入和配置，终端后端通过 C 编辑器接口按需解析。两者无需依赖运行时 crate。

## C 值、错误与回调

ABI 支持整数、浮点、布尔、UTF-8 字符串、可空字符串、字节数组和字符串数组；void 与 null 分开表示。指针和长度只在同步调用期间借用，接收方立即复制。Rust 容器、VmContext、托管地址和跨镜像 Rust trait 不经过 C 边界。

`HostApi` 提供参数读取、返回值复制、异常报告、脚本回调和逻辑库符号查询。native 状态仅由所属模块解释，运行时不解引用。Rust SDK 的 `dispatch` 捕获 panic，避免展开穿过 C 边界。返回值在 native 等待区间结束后才转换为托管值，错误类名和消息也复制保存。

函数显式声明 `BLOCKING` 与 `REENTERS`。阻塞调用前 park mutator；回调脚本前临时 unpark，并在执行期间固定转换后的托管参数根，返回后恢复 park。回调结果复制到当前调用的 arena；脚本抛出通过 `THROWN` 与 VM 异常槽传播。Repl 编辑回调按原契约把错误转为无动作；显式 completion probe 保留脚本异常。

## 平台能力与代码寿命

PAL 的 `dynlib::is_supported()` 汇合构建开关和 OS 能力。iOS、WASM 及关闭 native interop 的构建拒绝脚本 native 动态库加载，返回 `Unsupported`；静态解析器不受此限制。可信运行时组件使用独立的 component-loading 契约，不与脚本扩展能力混同。按路径打开的模块代码由 PAL 进程缓存保留，覆盖 VM 状态析构及线程本地析构入口。

Repl 的终端编辑要求平台终端能力和可用的后端链接方式；历史能力还要求文件系统。这些查询描述构建能力，不探测实际 tty 或安装文件。非终端平台、缺库或后端初始化失败走 plain 输入；持久历史属于终端后端。补全、键位回调名称和关键词配置按 VM 隔离。静态值成员查询属于核心反射，与终端模块无关。

## 当前边界与验证

带 `lib` 但不带 `type` 的脚本声明仍编译为按裸名解析的 builtin；通用桥在内部 builtin 查询未命中时解析唯一的 native 导出。已有 typed native 声明继续通过 Tier 1 的类型描述符、`CallNative` 和 libffi 调用。通用模块 ABI 的脚本回调已实现，旧 Tier 1 C 入口 `z42_invoke` / `z42_invoke_method` 仍未实现，二者不能混为一谈。

模块注册/调用由 `interop/providers_tests.rs`、`interop/module_tests.rs` 覆盖；真实动态镜像由 `loader/native_tests.rs` 覆盖；静态解析由 `loader/linked_tests.rs` 覆盖；Repl 会话与 SDK 编组分别在各自 crate 测试。完整验证使用 `xtask test` 和 `xtask package sdk --verify`。布局与搜索策略见[原生库布局](native-libraries.md)，跨平台产物及 WASM 链接模型见[组件化运行时](componentized-runtime.md)。
