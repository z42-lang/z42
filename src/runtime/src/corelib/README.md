# corelib/

## 职责

z42 VM 的**原生 builtin 层**：为 z42 标准库（`Std.*`）里那些无法用纯 z42 表达、
必须落到 Rust 的能力（IO、文件系统、反射、进程/线程/网络、加密、GC 控制…）提供
native 实现。builtin 一律写成类型化 Rust 函数（builtin ABI v2，`#[builtin]`，见下「加一个
builtin」），登记进 `BUILTINS` 表（`builtin_table.rs` 把各 `#[builtins]` 模块的描述符编译期拼接），
由解释器 `Instruction::Builtin`、JIT `jit_builtin` 和 `[Native]` 包装函数的折叠调用经
`invoke_builtin` 调用——实参原地从调用方寄存器读。

**不做**：语言语义（在 `interp` / `jit`）、类型/字节码元数据（在 `metadata`）、
GC 堆本身（在 `gc`）。凡能用纯 z42 写的（StringBuilder / List / Assert 等）都已从
这里下沉到 stdlib 脚本，corelib 只保留真正需要 Rust 的最小面。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| builtin 注册表 + 名称↔id 解析 + 分发 | `mod.rs` 的 `invoke_builtin`（引擎）/ `exec_builtin`（按名，测试/嵌入/ToString 回落）；`builtin_table.rs` 的 `BUILTINS` 表（`TYPED` 列出 `#[builtins]` 模块，一行一个） |
| builtin ABI v2：类型化实参 / 返回值、`BuiltinError` 与异常类、`blocking` | **`abi/`**（`args.rs` / `ret.rs` / `error.rs`；宏在 `crates/z42-macros/src/builtins_attr.rs`） |
| 值转换 / 解析（`Parse` / `TryParse`）/ to-string / 装箱 | `convert.rs`（`typed` 模块，ABI v2） |
| 控制台 IO（print / readline：`typed` 模块，ABI v2；写进程 stdout / stderr 与读 stdin 都 park）/ TestIO 捕获（sink 栈） | `io.rs` |
| 字符串操作 / 字符（含 bulk 原语 `__str_to_chars` / `__str_substring` / `__str_concat_parts` / `__str_split` / `__str_join`：Substring / StringBuilder.ToString / Join / Concat / Split 的底座，一次拷贝取代逐字符 builtin 派发）| `string.rs` / `str_meta.rs` / `char.rs` |
| 数学 | `math.rs` |
| 文件系统 / FileStream 句柄 / env / 时间（`typed` 模块，ABI v2：.NET 口径的 IO 异常 `fs_error`、文件 I/O 一律 park、句柄表锁外做系统调用；路径型操作走本 VM 的 `pal::fs::Vfs` 句柄 `VmContext::vfs`，返回 `io::Error`） | `fs.rs` |
| 对象内建（GetType / RefEq / HashCode） | `object.rs` |
| **反射**（`Std.Type` / `Std.Reflection.*`：枚举成员、attribute、反射调用、Activator、模块加载） | **`reflection/`**（子目录，见下「核心文件」） |
| 值类型 struct 字段布局复现（反射读写内联/装箱 struct） | `struct_reflect.rs` |
| 程序集加载上下文（`AssemblyLoadContext`） | `assemblyloadcontext.rs` |
| 运行时诊断 / 计数器 / profile | `diagnostics.rs` |
| 运行时设置只读查询 / 应用自定义属性 | `config.rs`（`Std.Runtime.RuntimeConfig`）/ `appprops.rs`（`Std.Runtime.AppProperties`） |
| `available!(X)` 运行期回落 | `symavail.rs` |
| 数组内建 | `array.rs` |
| GC 控制（collect / 阈值 / 快照） | `gc.rs` |
| 基准计时 | `bench.rs` |
| 子进程（`typed` 模块，ABI v2：失败从 Rust 抛、等子进程一律 park）/ 平台 / 系统信息 | `process.rs` / `platform.rs` / `system.rs` |
| 线程 / 同步原语底座 Monitor（不含值，Mutex/RwLock/Channel 由 z42 写在其上；`typed` 模块，ABI v2：误用抛 `LockRecursionException` / `SynchronizationLockException`，争用时的等待 park）/ 锁争用探针 | `threading.rs` / `monitor.rs` / `sync_contention.rs`；机制见 [sync-primitives.md](../../../../docs/internals/src/runtime/sync-primitives.md) |
| 网络（ABI v2：`__tcp_*` / `__udp_*` / `__dns_lookup`，.NET 口径的 `SocketException` 等从 Rust 抛、会阻塞的一律 park）/ 种子兼容的旧 `__net_*` 数组形状 | `network.rs`（hub）+ `network/{tcp, udp, error, listener, unsupported}.rs` / `network/legacy.rs` + `network/legacy/` |
| TLS（ABI v2：`__tls_*`，握手被拒 → `AuthenticationException`、传输 / 流 → `IOException`）/ 加密 | `tls.rs` / `crypto.rs` |
| REPL 支持（求值宿主 / 行编辑 / 行编辑器 cdylib 的懒 dlopen） | `repl.rs` / `repl_editing.rs` / `repl_native.rs` |
| 测试宿主（隔离跑 golden） | `tests.rs`（+ `reflection/module_load.rs` 的 `__run_goldens_isolated`）|

## 基础用法

**加一个 builtin**（新增一条需要 Rust 的 native 能力）——一律用 builtin ABI v2：

1. 在对应类别文件的 `#[z42_macros::builtins]` 模块里写一个**类型化函数**（没有就建一个 `pub mod typed`）：

   ```rust
   #[z42_macros::builtins]
   pub mod typed {
       use crate::corelib::abi::BuiltinError;
       #[builtin(name = "__foo_bar", pure)]
       pub fn foo_bar(s: &str, n: i64) -> Result<String, BuiltinError> { … }
   }
   ```

   实参类型（`&str` / `i64` / `f64` / `bool` / `char` / `Option<&str>` / `&Value` / `RefArg` …）由宏生成的包装器
   直接从调用方寄存器读，null / 类型不符报类型化异常；失败返回 `BuiltinError` 的对应种类（→ .NET 同名异常类），
   **VM 自身的缺陷**才用 `BuiltinError::Internal`（致命，不可 catch）。会阻塞的 builtin 加 `blocking`
   （整段 park 运行，只收发纯 Rust 数据）或返回 `Blocking::new(..)`；会执行 z42 代码的加 `reenters`。
   完整约定见 internals「native ABI §6」。
2. 新建的 `#[builtins]` 模块在 `builtin_table.rs` 的 `TYPED` 里加**一行**（模块的 `DESCS`）——之后模块里每加一个
   `#[builtin]` 函数就自动登记，无需再碰表。
3. z42 侧用 `[Native("__foo_bar")]` 声明对应外部函数（stdlib）。

**改语义要换名字**：已发布种子的 stdlib 会跑在新 VM 上，同名 builtin 的签名与返回形状必须兼容
（只把抛出的异常换成更具体的子类是兼容的）。z42.core 里被别的包调用的 `[Native]` 包装（`ProcessNative`、
`ThreadNative`、`TcpSocketNative` / `UdpSocketNative` / `TlsSocketNative` / `DnsNative`）同理——加新方法 / 新类，不改旧方法的形状；
种子不再声明旧名后，core 侧声明与运行时表项、legacy 实现一并删除（native ABI §6.5）。

**lenient 约定**：类型/反射类 builtin 对「无 handle 的合成 Type（数组 `T[]`，及 z42.core
未加载时的 primitive 兜底）」或非预期入参一律返回空数组 / null，不 `bail!`。primitive 正常解析为真 `Std.*` 句柄（`typeof(int)` ≡ `(5).GetType()`）。

## 如何测试验证

```bash
# corelib Rust 单元测试（各 <mod>_tests.rs，含 reflection/reflection_tests.rs）
(cd src/runtime && cargo test --lib)

# [Native("__x")] 声明 ↔ BUILTINS 双向对账（native_decl_tests.rs）：
#   扫 src/libraries/**/*.z42；声明了但表里没有 → 红；表里有但未声明且不在 UNDECLARED_ALLOWLIST → 红。
#   加 builtin 时：要么在 stdlib 声明，要么进 allowlist 并写明原因（编译器直接发射 / VM 内部 / host-only）。
(cd src/runtime && cargo test --lib native_decl)

# builtin ABI v2 本身：宏生成的读参 / 错误映射 / blocking（abi_tests）与 [Native] 桩折叠（interp/forward_tests，含 JIT 对照）
(cd src/runtime && cargo test --lib -- abi_tests forward_tests)

# 反射等端到端 golden（src/tests/reflection/ 等）
xtask test e2e

# stdlib [Test] dogfood（Std.Reflection / Std.IO / … 真实调用 builtin）
xtask test stdlib
```

## 关联文档

- 设计/机制（深入层）：[反射机制](../../../../docs/reference/src/stdlib/core.Type.md)、
  [运行时 IR](../../../../docs/internals/src/formats/ir.md)
- 反射能力的引入/演进：`git log -- src/runtime/src/corelib/`（需求↔迭代可追溯）

## 核心文件

| 文件 | 职责 |
|------|------|
| `mod.rs` | 名称↔`BuiltinId` 解析 + `invoke_builtin` / `exec_builtin` 分发 |
| `abi/` | builtin ABI v2：`BuiltinDesc` / `FromArg` / `IntoRet` / `Blocking` / `BuiltinError` + `raise`（两引擎共用的失败出口）；`test_builtins.rs` 是 `cfg(test)` 的测试用 builtin |
| `builtin_table.rs` | `BUILTINS`：各 `#[builtins]` 模块的 `DESCS` 经 `abi::concat_descs` 编译期拼接（下标即 `BuiltinId`，不跨进程持久化） |
| `reflection/mod.rs` | 反射 hub —— 导入 + 共享常量 + 私有 re-glob 各 concern 子模块（兄弟经 `use super::*` 互见）+ `pub use` 保留公开 API |
| `reflection/type_object.rs` | `Std.Type` 对象构造（`make_type_object` / `make_type_from_name` / `make_constructed_type`）+ handle/slot 辅助 |
| `reflection/type_query.rs` | 类型查询谓词（base / interfaces / members / nested / is_abstract·sealed·value·record·interface·class·primitive·generic / assignable_from / visibility）|
| `reflection/fields.rs` | 字段枚举（`__type_fields` + `FieldInfo` 构造，含继承静态字段）|
| `reflection/methods.rs` | 方法/构造函数枚举（`__type_methods` / `__type_constructors` + `MethodInfo`/`ParameterInfo` 构造 + 签名解析）|
| `reflection/properties.rs` | 属性枚举（`get_`/`set_` 访问器归并为 `PropertyInfo`）|
| `reflection/attributes.rs` | custom-attribute 反射（type/method/field/property/param → 调工厂函数实例化）|
| `reflection/generics.rs` | 运行期泛型实例化（`MakeGenericType` + `where` 约束校验）|
| `reflection/enums.rs` | 枚举反射（names/values/name/parse/is_defined/underlying）|
| `reflection/invoke.rs` | 反射调用（`MethodInfo.Invoke` / `MakeGenericMethod` / `Activator.CreateInstance` / `ConstructorInfo.Invoke` / `__invoke_static`）+ slot 读取 |
| `reflection/accessors.rs` | 反射读写字段/属性值（含内联/装箱 struct 的 byte-region 读写 + 写屏障）|
| `reflection/methodof.rs` | `methodof(Type.Member(sig))` 运行期支持（编译期已绑定重载，运行期按签名取 `MethodInfo`） |
| `reflection/module_load.rs` | 运行期加载模块/字节码（REPL、test 宿主）+ `__run_goldens_isolated` |
| `convert.rs` / `io.rs` / `string.rs` / `math.rs` / `fs.rs` | 各类别 builtin 实现（见「功能索引」）|
| `struct_reflect.rs` | 值类型 struct 字段布局复现（供 `reflection/accessors.rs` 读写内联/装箱 struct）|
| `<mod>_tests.rs` | 各模块 Rust 单元测试（`reflection` 的在 `reflection/reflection_tests.rs`）|
