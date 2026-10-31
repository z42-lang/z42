# Native interop ABI（Tier 1 运行时）

> **相关**：[对象与值表示 ABI](object-abi.md) · [嵌入宿主](embedding.md) ·
> [Native 扩展库范式](native-extensions.md)
>
> 用户侧契约见参考手册 [Native 互操作（FFI）契约](https://z42-lang.github.io/z42/reference/embedding/native-interop.html)。
> 本页讲 VM 内部**怎么实现**它、为什么这么分层、哪些部分还没通电。

## 与邻页的边界

「native」这个词在本仓里覆盖五条互不相同的通路，落在五页上：

| 页 | 讲的是 |
|---|---|
| **本页** | **typed native type** 通路：`Z42TypeDescriptor_v1` 注册 → `CallNative` → libffi |
| [native-extensions.md](native-extensions.md) | 独立 cdylib 的 C ABI 范式（compression / repl 两个实例、回调 trampoline） |
| [native-ext-loader.md](native-ext-loader.md) | 那些 cdylib 怎么被发现、符号怎么注册进 `ext_builtins` |
| [native-libraries.md](native-libraries.md) | native 库住哪个目录、发布期怎么拍平 |
| [embedding.md](embedding.md) | 反方向：宿主启动并驱动 VM（`z42_host.h`） |

判据是 `[Native(...)]` 里**有没有 `type=`**：有 → 本页的 typed 通路；没有 → builtin 名字解析，落
`BUILTINS[]` 或 ext 表（§6）。两条通路在编译器里由同一处分流（`StubEmitter._emitNativeStub`），在 VM
里则是完全不同的两套代码。

`Z42Value` / `Z42Args` / `Z42Error` 三个值形状由本通路与 embedding 通路**共用同一份定义**
（`z42_abi.h` 与它的 Rust 镜像 crate），`z42_host.h` 不重复声明——见 [embedding.md §4.1](embedding.md)。

---

## 1. 三层架构

```
┌──────────────────────────────────────────────────────────────┐
│  Tier 3: 源生成器（编译期绑定）              —— 未实现        │
│    编译器读 manifest → 发 IR vtable → CallNativeVtable       │
├──────────────────────────────────────────────────────────────┤
│  Tier 2: Rust 注册 API（人因工程）          —— 部分实现      │
│    #[z42::methods] + z42::module!                           │
│    proc macro 展开出 Tier 1 描述符 + extern "C" 壳           │
├──────────────────────────────────────────────────────────────┤
│  Tier 1: C ABI（稳定基座）                  —— 可用          │
│    Z42TypeDescriptor_v1 + z42_register_type                 │
│    跨语言可移植；上面两层最终都落到这里                        │
└──────────────────────────────────────────────────────────────┘
                              ↓
                    VmCore.native_types
```

**分层原则**：上层能表达的，手写下层都能得到。由此保证三件事——Tier 1 保持小而稳定；Tier 2/3 可以独立
演进而不破坏 Tier 1 的消费者；没有 Rust 的语言（C、C++、Zig、Go cgo）直接对着 Tier 1 写。

### 1.1 为什么要「native 定义完整脚本类型」

这是本设计相对 C# 的关键分歧点。对照三家：

| 能力 | Python C API | C# P/Invoke | z42 的目标形态 |
|---|---|---|---|
| 脚本能调 native 函数 | ✓ | ✓ | ✓（已实现） |
| native 能定义完整类型 | ✓（`PyTypeObject`） | ✗（只有不透明 `IntPtr` 包装） | 目标 ✓（未实现） |
| 该类型参与 `is` / 类型判定 | ✓ | ✗ | 目标 ✓（未实现） |
| native 实现脚本侧接口 | ✓（鸭子类型） | ✗ | 目标 ✓（vtable，未实现） |
| ABI 稳定性 | ⚠ 偶有破坏 | N/A | ✓ 版本化 |

`Z42TypeDescriptor_v1` 从一开始就带 `fields` / `trait_impls` 两段，就是为这个目标留的位置。今天 VM
**解析它们但不消费**（`RegisteredType` 只存 `methods`），实际可用的仍是句柄模式——差距的形状见 §8.5。

### 1.2 为什么定编译期解析

参考 C# 11+ `[LibraryImport]` 对反射式 `[DllImport]` 的取代：

| 维度 | 运行期 `dlopen`+`dlsym` | 编译期解析 |
|---|---|---|
| AOT 兼容 | 差 | 原生 |
| 首次调用开销 | dlsym + 签名查找 | 零 |
| 签名不符 | 运行时崩 | 编译错误 |
| 类型缺失 | 运行时崩 | 编译错误 |
| ABI 版本不符 | 运行时 panic | 编译错误 |

⚠️ **这条决策目前只兑现了一半**。编译器不读任何 native 元数据——`[Native(lib=,type=,entry=)]` 里的三个
串被原样烤进 `CallNativeInstr`，全部校验推迟到注册期（签名串解析）与调用期（类型/方法查找、marshal）。
表右列描述的是 Tier 3 落地后的形态，不是现状。

---

## 2. 代码归属

| 路径 | 内容 |
|---|---|
| [`src/runtime/include/z42_abi.h`](https://github.com/z42-lang/z42/blob/main/src/runtime/include/z42_abi.h) | Tier 1 C 头（与 `z42_host.h` 平行） |
| [`src/runtime/src/native/`](https://github.com/z42-lang/z42/tree/main/src/runtime/src/native) | Tier 1 在 VM 内的实现（`registry` / `marshal` / `dispatch` / `loader` / `error` / `exports` / `ext`） |
| [`src/runtime/crates/z42-abi`](https://github.com/z42-lang/z42/tree/main/src/runtime/crates/z42-abi) | Tier 1 的 Rust 镜像（结构体 + 冻结 tag 常量 + `extern "C"` 声明） |
| [`src/runtime/crates/z42-rs`](https://github.com/z42-lang/z42/tree/main/src/runtime/crates/z42-rs) | 面向用户的门面 crate（`Z42Type` trait + helper） |
| [`src/runtime/crates/z42-macros`](https://github.com/z42-lang/z42/tree/main/src/runtime/crates/z42-macros) | proc macro：`methods_attr` / `module_macro` / `shim` / `signature`（Tier 2）；`builtins_attr`（VM 自己的 builtin ABI，§6.2） |
| `src/compiler/z42c.emission/src/Emission/StubEmitter.z42` | 编译器侧：`[Native]` → `CallNativeInstr` / `BuiltinInstr` 的分流 |

整条通路由 cargo feature **`native-interop`** 门控（`= ["dep:libffi", "dep:libloading"]`，在默认
feature 集里）。wasm 构建关掉它后，`VmContext` 上的 `register_native_type` / `resolve_native_type`
连同背后的字段一起消失。

### 2.1 宏各自展开出什么

| 宏 | 展开产物 |
|---|---|
| `#[z42::methods(module=, name=)]` | 每方法一个 `extern "C"` 壳 + `Z42MethodDesc` 条目；合成 `alloc` / `dealloc` / `ctor` / `dtor`；拼出 `static Z42TypeDescriptor_v1` |
| `z42::module! { name:, types: [..] }` | `#[no_mangle] extern "C" fn <name>_register()`，逐个调 `z42_register_type(<T as Z42Type>::descriptor())` |
| `#[derive(Z42Type)]` | **占位**：展开成 `compile_error!` |
| `#[z42::trait_impl]` | **占位**：展开成 `compile_error!` |

宏侧的 Rust 类型白名单（`signature.rs::parse_type`）比 ABI 侧更窄：只有基元、裸指针、`Self` 引用与
`()`；`&T` / `&str` / `String` / `Vec<T>` / `Box<T>` / `Result<T,E>` / 按值 `self` 都给出定位到具体
token 的 `syn::Error`。渲染出的签名串必须与 `dispatch::parse_signature` 往返一致——**两处白名单是一对
必须同步的表**，改一边就要改另一边。

---

## 3. 注册路径

```
native 库                      z42vm
  <libname>_register()  ──▶  exports::z42_register_type
                               ↓  校验 abi_version
                             registry::RegisteredType::from_descriptor
                               ↓  读 methods[]，逐个 parse_signature
                               ↓  预建 libffi Cif（每方法一份）
                             VmContext::register_native_type
                               ↓  key = (module_name, type_name)
                             VmCore.native_types: RwLock<HashMap<..>>
```

要点：

- **签名在注册期就解析完**。签名串写错在 `z42_register_type` 处失败（返回 `NULL` + 写 thread-local
  last-error 槽），不会拖到调用时才炸。`Cif` 预建也让调用路径上没有签名解析开销。
- **`(module, type)` 重复注册直接拒绝**（返回 `false`），不做覆盖——避免"后注册的静默赢"。
- **描述符指针只留作诊断**。`RegisteredType` 注册完就不再回读 `descriptor_ptr` 指向的内存；调用方必须
  保证描述符及其所有字符串/函数指针是 `'static`。这条契约是 `unsafe impl Send + Sync` 的全部依据。
- **`fields` / `trait_impls` 不落库**。`RegisteredType` 只存 `methods: HashMap<String, MethodEntry>`。
- **库句柄挂在 `VmContext.native_libs` 上**，保证所有 `fn_ptr` 活到 VM drop。

`loader::load_library` 按文件名反推入口符号：`libnumz42.dylib` → `numz42_register`。⚠️ **VM 内部没有
任何地方调 `load_native_library`**——typed 通路的库今天只能靠静态链接 + 宿主显式调注册入口进来。启动时
被自动扫描 `dlopen` 的是 ext 通路（§6.4），两者不要混。

---

## 4. 调用路径

### 4.1 `CallNative`

`CallNativeInstr(dst, module, type, symbol, args)` 由编译器发射（`StubEmitter`），interp 在
`exec_native::call_native` 消费：

```
计数 native_calls + 发 NativeCallEntered 事件
  → ctx.resolve_native_type(module, type)        未命中 → unknown native type
  → ty.method(symbol)                            未命中 → unknown method
  → 实参个数 vs 签名参数个数                       不符  → arity mismatch
  → marshal::Arena::new()                        持有本次调用的临时（CString 等）
  → 逐参 marshal::value_to_z42(v, &param_ty, &mut arena)
  → dispatch::call(cif, fn_ptr, z_args, params, ret)   ← libffi
  → drop(arena)                                  临时立即失效
  → marshal::z42_to_value(z_ret, &ret_ty) → frame.set(dst)
```

计数器在**分派之前**自增：失败的调用（未知类型 / marshal 错）同样是 FFI 流量，也要计进去。

### 4.2 marshal 的两类错误

`MarshalErr` 分两支，决定错误怎么冒出去：

- **`InvalidMarshal`** —— 用户可捕获，构造成 `Std.InvalidMarshalException` 经 interp 的值型 throw 通道
  返回（`Ok(Some(exc))`）。当前只有一个来源：`string` 含内部 NUL 无法做成 `*const c_char`。
- **`Internal`** —— `anyhow!` 错误，不可捕获。类型对不上 blittable 子集、`PinnedView` 直接进 marshal 等
  都走这支。

### 4.3 引擎覆盖

**只有解释器实现了这条通路。** `CallNative` / `CallNativeVtable` 都在 JIT 的 `unsupported_reason` 名单
里：含这两条指令的函数整体不进 JIT，回落解释执行（`jit/translate/unsupported.rs`）。`PinPtr` /
`UnpinPtr` 同理。

这不是暂时的实现懒惰——JIT 直接发 `call` 需要先有 Tier 3 的 vtable 槽解析（§8.1），而那一层还不存在。
在此之前"JIT 发直接调用、AOT 由链接器解析"只是目标形态。

---

## 5. `PinnedView` 的运行期表示

`PinPtr` / `UnpinPtr` 两条 opcode 在 interp 里已实现，产出 `Value::PinnedView`。

句柄模型本身归 [object-abi.md §2.2](object-abi.md)：`PinnedView` 是四个瞬态 arena 句柄之一，寄存器里
只有 8B 的 `{ idx: u32, frame_id: u32 } = 8`，真载荷 `PinnedViewData { ptr: u64, len: u64, kind:
PinSourceKind }` 住在 per-`VmContext` 的 `TransientArena` 里，随帧 LIFO 截断。本节只讲 `PinnedView`
独有的部分。

**两种源，两种语义**：

| 源 | 语义 | VM 侧持有物 |
|---|---|---|
| `Value::Str` | **借用**源字符串的字节 | 无；`UnpinPtr` 是 no-op |
| `Value::Array`（元素为 `u8`） | **快照**：拷出一份 `Box<[u8]>` | `VmCore.pinned_owned_buffers: Mutex<HashMap<u64, Box<[u8]>>>`，`UnpinPtr` 按 ptr 移除 |

数组路径对 packed `byte[]`（`Bytes` 背衬）直接 `as_bytes().to_vec()`；boxed 背衬才逐元素校验
`Value::I64 ∈ 0..=255`，越界抛 `Std.InvalidMarshalException`。因为是快照，pin 期间对源数组的写**不会**
被 native 侧看到——与 `Str` 的借用语义不同，这条不对称是有意的：VM 没有办法把 boxed 元素数组原地变成
连续字节。

**字段投影**：`view.ptr` / `view.len` 走标准 `FieldGet`，在 `exec_object::field_get` 里经 arena 解句柄
取真载荷。`marshal::value_to_z42` 拿不到 `ctx`，所以它对 `PinnedView` 的臂是**明确报错**而非兜底——
调用点必须先 `FieldGet` 投成标量再传。这是 object-abi §2.2「ctx-less 消费者对 arena 载荷降级」那条通用
决策在本通路上的落点。

**当前没有生产者**：`pinned` 是保留关键字，但 z42c 侧既没有语句解析也没有 `PinPtr` 的发射点，
`z42.package` 里连对应的指令类都没有。这两条 opcode 今天只由手写 IR 的测试驱动（§8.3）。

---

## 6. 内置函数通路（`BUILTINS`）

`[Native("__name")]` 把 stdlib 的 z42 声明接到 VM 里的 Rust 实现上。builtin 一律写成
**ABI v2 的类型化 builtin**（`#[builtin]`，§6.2）——签名即实参/返回/错误的类型；旧表
（手写 `fn(&VmContext, &[Value])`）已随 P1-8 批次 12b 全部迁完删除。

### 6.1 表与 id

```rust
const TYPED: &[&[abi::BuiltinDesc]] = &[convert::typed::DESCS, string::typed::DESCS, …];  // 一个模块一行
const JOINED: [abi::BuiltinDesc; N] = abi::concat_descs(TYPED);  // 编译期拼接
pub(crate) const BUILTINS: &[abi::BuiltinDesc] = &JOINED;
```

**slice 下标就是 `BuiltinId`。**

> `BuiltinId` **不会被烤进 zbc**：zbc 里存的是**名字**（`BuiltinInsn { dst, name, args }`），
> `BuiltinId` 由 resolver 在**加载期**按名字填进 `Function.resolved.builtin_tokens`，AOT 也不烤它
> ⇒ 它是**单次运行内的派发令牌**，不跨进程持久化——增删表项只让下标平移，对既有产物没有影响。

- 名字唯一：同一个名字只能在表里出现一次（`abi_tests::typed_builtins_are_in_the_table_once`）。
- 名字 → id 的反查表 `BUILTIN_INDEX: OnceLock<HashMap<&str, u32>>` 首次访问时从 `BUILTINS` 现算
  （`corelib/mod.rs`），保证单一真相。

加载期由 `metadata::resolver` 把每个 builtin 调用点的名字解析成 token：先查 `BUILTINS`，未命中再查
per-VM 的 ext 表；两边都没有则留 `UNRESOLVED`，在真正调用时按名字再解一次（JIT 在 ext 库尚未加载时就
可能走到这里）。调用时仍解不出 ⇒ 抛 `Std.MissingSymbolException`（与调用不存在的函数同一个类）。

**两个引擎的调用口只有一个**：`corelib::invoke_builtin(ctx, id, name, this, regs, args)`。实参
**原地从调用方寄存器读**——`this`（vcall 短路持有的接收者，不在寄存器里）在前，其后是 `regs[args[i]]`。
包装器直接按下标取实参。解释器 `exec_call::builtin` 与 JIT `jit_builtin`（签名
`(frame, ctx, dst, id, name, args_ptr, argc)`）都只做「结果写 `dst`、失败交给 §6.3」。只握着切片的
调用方（单测、嵌入、`exec_builtin_value`）走 `exec_builtin`（按名），类型化 builtin 为它们另生成一个
读切片的入口；**ext** builtin（dlopen / bundled，`fn(&VmContext, &[Value])`）的实参才拷进栈上缓冲区
（≤ 6 个，超出用 `Vec`）——缓冲区原地填、不移动，实参逐个 16 字节拷贝（经 `Option<Value>` 中转或移动
`SmallVec` 会让被调方读实参时错过 store forwarding，曾使折叠后的 ext 调用慢 25%）。

### 6.1b 有返回值 vs 无返回值

**void builtin 不用 `Value::Null` 表示「没有返回值」**：调用口返回 `Option<Value>`，**`None` = void**，
interp / JIT 都**不写 `dst`**（若共用 `Value` 返回值，「无返回值」与「返回 null」在寄存器里一模一样）。

| 部件 | 契约 |
|---|---|
| 类型化 builtin | 返回 `()` ⇒ `RetKind::Void`；其余返回类型都产出值 |
| 期待有值的调用方 | `exec_builtin_value(..) -> Result<Value>` —— 撞上 void **报错**，不给占位值 |

🔒 **有门**：`corelib/native_decl_tests.rs::declared_voidness_matches_the_builtin_table` 对账
「stdlib 声明 `void` ⟺ 表项无返回值」，`every_declared_native_has_a_builtin` 对账名字。参数个数与类型
**没有编译期对账**（User 决定：靠测试——声明与 Rust 签名不一致，调用即报内部错误，见 §6.3）。

### 6.2 加一个 builtin（ABI v2：`#[builtin]`）

builtin 写成**普通的类型化 Rust 函数**，放进所在领域文件里的一个 `#[z42_macros::builtins]` 内联模块：

```rust
#[z42_macros::builtins]
pub mod typed {
    use crate::corelib::abi::{BuiltinError, RefArg};
    use crate::vm_context::VmContext;

    #[builtin(name = "__int32_parse", pure)]
    pub fn int32_parse(s: &str) -> Result<i64, BuiltinError> { … }

    #[builtin(name = "__str_char_at", this = "CharAt", pure)]
    pub fn str_char_at(this: &Str, i: i64) -> Result<char, BuiltinError> { … }

    #[builtin(name = "__int32_try_parse")]
    pub fn int32_try_parse(ctx: &VmContext, s: Option<&str>, v: RefArg) -> Result<bool, BuiltinError> { … }

    #[builtin(name = "__thread_sleep", blocking)]
    pub fn thread_sleep(millis: i64) { … }
}
```

三步：

1. 在领域文件的 `#[builtins]` 模块里写函数（没有就建一个 `pub mod typed`）；
2. 新模块要在 `builtin_table.rs` 的 `TYPED` 里加**一行**（模块的 `DESCS`）——模块里的函数不再逐个登记，
   写了 `#[builtin]` 就在表里；`#[builtin]` 写在 `#[builtins]` 模块之外是编译错误（找不到该属性）；
3. z42 侧用 `[Native("__foo")] … extern` 声明（stdlib）。

宏（`crates/z42-macros/src/builtins_attr.rs`）是**单一真相**，每个函数展开出：

- 一个按参数源泛型的读参包装器，实例化两份：`__z42b_regs_<f>`（从寄存器按下标读，引擎用）与
  `__z42b_slice_<f>`（读 `&[Value]`）；
- 一个 `BuiltinDesc { name, params: &[ParamKind], ret: RetKind, flags, call_regs, call_slice }`；
- 模块尾部的 `DESCS`（编译期拼接；带 `#[cfg]` 的 builtin 随条件整条消失）。

**签名约定**（`corelib/abi/`）：

| 位置 | 能写什么 | 说明 |
|---|---|---|
| 首参 | `ctx: &VmContext`（可选） | 不算 z42 实参 |
| 参数 | `&str` / `String` / `&Str`、`Option<&str>` / `Option<String>`、`i64`、`f64`、`bool`、`char`、`&Value` / `Value`、`RefArg`、`Vec<u8>` / `Option<Vec<u8>>`（`byte[]`，拷出）、`Vec<String>`（`string[]`，拷出） | 每种一个 `FromArg` impl：null / 类型检查在这里，失败是类型化错误 |
| 返回 | `T` 或 `Result<T, BuiltinError>`，`T` ∈ `()`、`i64`、`f64`、`bool`、`char`、`String`、`&str`、`Option<String>`、`Value`、`Vec<u8>`（新的打包 `byte[]`）、`Vec<String>`（`string[]`）、`Blocking<R>`、`Then<V>`，或领域自己的 `IntoRet` 类型（`process::RunOutput` 建结果数组） | 每种一个 `IntoRet` impl；`()` = void |
| 属性 | `name = "__x"`（必填）、`pure`、`blocking`、`reenters`、`this = "Member"` | 见下 |

- **实参错误**：非空参数收到 null → `ArgumentNullException("Value cannot be null. (Parameter 's')")`
  （参数名取自 Rust 形参）；`this = "Member"` 标出的接收者为 null → `NullReferenceException`，消息与
  `VCall` 撞 null 同一条；类型不符 → `InvalidCastException`；**实参个数不符**只可能是 `[Native]` 声明与
  Rust 签名不一致 ⇒ `Internal`（致命）。
- **`ref` / `out` 出参**：参数写 `RefArg`，`v.store(ctx, value)` 经 transient arena 解出位置后写穿
  （与 z42 方法里给 `ref` 形参赋值同一条 `store_thru_ref`）。`TryParse` 一族就是这样做到不抛异常的。
  builtin 拿到调用方的引用依赖 §6.4 的折叠（直接调用与虚调用都会折叠）：真正执行的包装函数帧在入口把 `ref`
  实参解引用成值（copy-in / copy-out），builtin 写不回去；这种情况只出现在反射 `Invoke` / 嵌入方直接调用，
  而它们本来就只传值、没有 `ref` 位置——`store` 此时什么都不做，与不折叠时的效果一致。
- **`pure`**：无副作用、结果只取决于实参（描述符数据，供日后 JIT intrinsic 使用）。
- **`reenters`**：builtin 可能执行 z42 代码（回调、getter、初始化器）。影响 §6.4 的折叠方式。

#### `blocking`：park 在结构上禁止分配

会长时间不返回的 builtin（sleep、join、网络 / 管道 I/O、条件等待）必须在阻塞期间 park，否则
GC 等不到它到达 safepoint（规则见 runtime-rust.md「阻塞的 native 调用必须 park」）。ABI v2 把规则做成结构：

- `#[builtin(blocking)]`：**整个函数体在 park 状态下运行**。它**不能**取 `&VmContext`（宏报错），参数
  必须是 `PlainArg`（`i64` / `f64` / `bool` / `char` / `String` / `Option<String>`，不指向 GC 堆），
  返回值必须是 `PlainRet`；包装器先把实参转成纯 Rust 数据、再 park、跑函数体、unpark，**最后**才把结果
  转成 `Value`（`String` 结果是一次 GC 分配）。函数体拿不到堆，就不可能在 park 期间分配。
- 需要先用 ctx 准备（从 registry 取出句柄）的 builtin 不加 `blocking`，而是返回 `Blocking::new(move || …)`：
  准备阶段 unpark 运行，闭包在 park 状态下运行，结果同样 unpark 之后才转换。闭包要求 `Send + 'static`
  ——借不到 `&VmContext`，也借不到调用方栈上的任何东西。`__thread_wait` / `__process_wait` 是这一形态的样板。
- `Blocking::ready(r)`：答案不用等（零长度读、已关闭的管道）——不 park。
- `Blocking::new(..).then(|ctx, r| …)` → `Then<V>`：parked 部分产出的不是 `PlainRet`（打开的 `File`、spawn 出的
  `Child`、连上的 `TcpStream`、要拷进调用方 `byte[]` 的字节），`then` 在 **unpark 之后**拿着 ctx 收尾——把句柄登记进表、
  写 GC 数组、分配、核对阻塞期间句柄有没有被别的线程关掉。`__file_open` / `__file_read` / `__process_start` /
  `__tcp_accept` / `__tcp_read` 是样板。
- 先在 unpark 状态下试一把、不行再等的（`__monitor_enter`：重入检查、无争用快路径、短暂自旋）返回
  `Result<Blocking<()>, _>`：拿到了就 `Blocking::ready(())`（不 park），要等才 `Blocking::new`（闭包只握 `Arc<Monitor>`）。
- 函数体自己 park 的只剩一种：执行期间要以本 ctx 回调 z42 的（`__repl_readline`：行编辑器在读的中途回调补全器，
  回调里 `NativeUnparkGuard` 暂离 park）。它标 `reenters`、函数体显式 `NativeParkGuard`，结果在 park 结束后才分配。
- 资源表（`file_handles` / `processes`）的锁下**只取 / 克隆句柄**（`Arc<File>`、每条管道自己的锁），系统调用在
  锁外做：一个阻塞在读上的线程既不占表锁，也不挡回收。

单测照 `abi_tests::a_blocking_builtin_lets_a_collection_run_while_it_waits` 写：阻塞线程 `parked_count == 1`，
另一线程此时能发起并完成一次全堆回收，结束后回到 0。debug 构建下 park 期间分配会被
`debug_assert_not_native_parked` 当场拦下。

### 6.3 错误模型（`BuiltinError`）

类型化 builtin 以 `BuiltinError` 失败，每一种对应一个 stdlib 异常类（.NET 的名字与继承关系）：

| 变体 | 抛出的类（按序取第一个已加载的，最后兜底 `Std.Exception`） |
|---|---|
| `Format` | `Std.FormatException` |
| `Argument` / `ArgumentNull` / `ArgumentOutOfRange` | `Std.ArgumentException` / `Std.ArgumentNullException` → `ArgumentException` / `Std.ArgumentOutOfRangeException` → `ArgumentException` |
| `IndexOutOfRange` / `InvalidOperation` / `NotSupported` / `InvalidCast` / `NullReference` / `KeyNotFound` | 同名 `Std.*` 类 |
| `Overflow` | `Std.OverflowException` → `Std.ArithmeticException` |
| `Io` / `FileNotFound` / `DirectoryNotFound` / `PathTooLong` / `EndOfStream` | `Std.IO.IOException` 及其同名子类（`EndOfStreamException` 在 `z42.io`） |
| `UnauthorizedAccess` / `Timeout` | `Std.UnauthorizedAccessException` / `Std.TimeoutException` |
| `Exception { class, message }` | 任意类（包级异常，如 `Std.Threading.ThreadException`、`Std.Security.Authentication.AuthenticationException`） |
| `Detailed { classes, message, fields }` | 按序取第一个已加载的类，另外填公开字段（`Std.IO.ProcessTimeoutException` 的 `Program` / `TimeoutMs`、`Std.Net.Sockets.SocketException` 的 `SocketErrorCode` / `ErrorCode`，退到基类时跳过它没有的字段）。`BuiltinError::object_disposed(name, msg)` 用它造 `Std.ObjectDisposedException`（→ `InvalidOperationException`），带 `ObjectName` |
| `Thrown(Value)` | 已构造好的异常值，原样抛（回调里 z42 代码抛的） |
| `Internal(String)` | **致命错误**，不可 catch |
| `Legacy(anyhow::Error)` | `anyhow` 桥：类型化 builtin 包装旧 helper（`obj_get_type` 等）与 **ext** builtin 的错误（`objops::Throw` 按其类，其余 `Std.Exception`；typed 可下插恢复） |

`From<std::io::Error>` 按 .NET 的口径映射 OS 错误（不存在 → `FileNotFound`，无权限 →
`UnauthorizedAccess`，超时 → `Timeout`，提前 EOF → `EndOfStream`，平台不支持 → `NotSupported`，其余 → `Io`）；
`From<OpError>` 接 objops。**按路径操作的文件系统 builtin 不用它**，而用 `corelib::fs::fs_error(e, path, role)`：
手里有路径、知道要找的是文件还是目录，才分得出 `FileNotFound`（文件所在的目录存在）与 `DirectoryNotFound`，
消息也是 .NET 的原文（`Could not find file '<path>'.`）——完整对照在该函数的文档与
[reference · 文件与 IO 异常](https://z42-lang.github.io/z42/reference/stdlib/core.IOException.html)。进程的对照在 `corelib/process.rs` 抬头；
socket 的（OS 错误 → `SocketException` + Winsock 码 `SocketErrorCode`，超时 → `TimedOut`，解析失败 → `HostNotFound`，
关闭的句柄 → `SocketClosedException : ObjectDisposedException`）在 `corelib/network/error.rs`，TLS 的（握手被拒 →
`AuthenticationException`，传输 / 流 → `IOException`）在 `corelib/tls.rs` 抬头；Monitor 误用 → `LockRecursionException` /
`SynchronizationLockException`（`corelib/monitor.rs`）。

**唯一的出口** `corelib::abi::error::raise`，两个引擎共用（interp `exec_call::builtin` / 折叠调用，
JIT `jit_builtin` / `call_forward`），判定顺序：

1. 回调里 z42 代码抛出的原异常（`pending_thrown`）优先，原样抛出；
2. **致命错误正在展开**（`stack_guard::is_fatal`）⇒ 不构造任何异常，原错误继续向外传；
3. `Internal` ⇒ `stack_guard::fatal_error`：与栈溢出**同一套机制**——置 VM 的致命标志、记下
   `fatal error: internal error in builtin `__x`: …` 加 z42 调用栈，此后不再有 z42 handler 运行，
   `z42vm` 打印报告并以退出码 3 结束，嵌入 API 返回 `Z42_HOST_ERR_FATAL`；
4. 其余按类构造异常，并**在 builtin 处**填 `StackTrace`——两个引擎的 trace 因此逐字一致
   （JIT 以前要等异常回到解释器帧才填）。

「内部错误」指 VM / stdlib 自身的缺陷：`[Native]` 声明与 Rust 签名不一致、句柄失效、不变式被破
（如 `__box_prim` 在 debug 构建收到 `Null`）。用户代码能触发的失败一律用对应的异常类。
`Legacy(anyhow)` 不再对应旧表（已删），只剩两个用途：类型化 builtin 包装 anyhow 返回的旧 helper
（如 `obj_get_type`），以及 **ext** builtin 的 `fn(&VmContext, &[Value])` 错误通道（`objops::Throw`
按其类、其余 `Std.Exception`）。

### 6.4 `[Native]` 桩折叠（两个引擎，VM 链接期）

每个 `[Native("__x")] extern` 方法编译成一条指令的包装函数
（`%n = builtin __x(%0…); ret %n`）。按普通协议调用它要付一整个调用帧，只为把实参原样转给 builtin。
调用解析到这样的转发函数时，**两个引擎都直接从调用方寄存器跑它的 builtin**（`interp/forward.rs`）：

- 解释器：`exec_call::call`（静态调用，在模块初始化 / 类型初始化屏障之后）与 `exec_vcall::invoke`；
- JIT：`jit_call` / `jit_vcall` 的已编译目标（`FnEntry::forward`）与冷目标（经解释器的那条路）。

识别是**按函数**的、首次调用时算一次并缓存在 `Function::forward`：

1. 一块、无异常表、无泛型参数、全部形参按序转发的 `builtin` + `ret`（含 void 形态）；
2. 同一类型内再包一层的 `call G(%0…); ret`，`G` 是形态 1（`string` 索引器：`get_Item` → `CharAt` →
   `__str_char_at`）——同类型保证外层的类型初始化屏障已覆盖 `G`；
3. 返回 blob struct 的 sret 桩（`GCHandle.Alloc`）**不折叠**：它的函数体依赖帧的 struct arena，且不在热路径上。

**为什么不在编译器里折叠**：把桩折进调用方等于把 stdlib 私有的 builtin 名字烤进用户包，用户包从此绑死
某一版 VM 的 builtin 集合，包版本边界就破了。VM 链接期按函数识别没有这个问题。

**栈回溯不变**（用户不可见折叠）：

- 不会执行 z42 代码的 builtin（类型化且不 `reenters`）：**不建任何帧**。失败时才从函数现算包装函数
  那一行（链式形态两行）拼进 trace，结果与不折叠时逐字相同；
- 可能执行 z42 代码的（类型化的 `reenters`：反射调用、getter、加载器、REPL 回调）：在 builtin 运行
  期间压一个**轻帧**——只有包装函数指针、没有寄存器文件、什么都不执行——于是回调里抛出的异常、致命
  报告都看到与不折叠时一样的栈。这类 builtin 只折叠形态 1。（旧表的 `LEGACY_REENTERS` 名单已随旧表
  删除，`reenters` 由各 builtin 自己声明。）

帧策略在函数首次被调用、算折叠结果时一并定下，编进缓存的折叠令牌（builtin id + 「要轻帧」位），调用时不再查表。

单测 `interp/forward_tests.rs` 在手建模块上逐条比对「折叠 / 不折叠 / JIT」三份 trace；golden
`src/tests/exceptions/stack_trace_native_fold.z42` 覆盖真实 stdlib。

### 6.5 命名约定

新 builtin 名一律 `__<area>_<verb>[_<modifier>]`，`<area>` 是单个词、内部不带下划线：

| area | 领域 | 例 |
|---|---|---|
| `str` | `Std.String` 上的字符串操作 | `__str_char_at` |
| `char` | `Std.Char` | `__char_is_whitespace` |
| `int32` / `int64` / `double` / … | 基元的 parse / try_parse / to_string | `__int32_parse`、`__int32_try_parse` |
| `math` | `Std.Math.Math` 静态方法 | `__math_sqrt`、`__math_atan2` |
| `obj` | 通用对象协议 | `__obj_get_type`、`__obj_hash_code` |
| `file` | `Std.IO.File` 静态方法 | `__file_read_text` |
| `env` | 环境 / 进程 | `__env_get`、`__env_args` |
| `time` | 时钟 / 计时 | `__time_now_ms` |
| `process` | 宿主进程控制、子进程（`Std.IO.Process`） | `__process_exit`、`__process_run_sync`、`__process_wait` |
| `thread` | `Std.Threading.Thread` | `__thread_spawn`、`__thread_wait` |
| `dir` | `Std.IO.Directory` 静态方法 | `__dir_enumerate` |
| `tcp` / `udp` / `dns` / `tls` | `Std.Net.Sockets` 的 TCP / UDP / 名字解析 / TLS 客户端 | `__tcp_connect`、`__udp_recv_into`、`__dns_lookup`、`__tls_read` |
| `monitor` | 同步原语底座 `MonitorNative` | `__monitor_enter` |

新领域 → 往这张表里加一行，再挑一个短单词。

**裸名一族**（`__println` / `__print` / `__readline` / `__to_str` / `__time_now_ms` /
`__process_exit`）不遵守该约定，**不要再加新的**（`__concat` / `__contains` / `__len` 三个零引用
旧名已随旧表删除）。

**改语义要换名字**：已发布的种子 stdlib 跑在新 VM 上（冷启动自举），同名 builtin 的签名与返回形状
必须保持兼容（只把抛出的异常换成更具体的子类是兼容的）。要改形状就起新名字（例：返回 `[KIND, …]` 判别数组的
旧形状 → 返回普通值、失败从 Rust 抛的新名字），旧名字挂 `STAGE2-DEBT` 留到种子不再声明它为止、随后连同表项
与实现一并删除。形状不变、只把抛出的异常换成更具体子类的可以原名迁（`__monitor_*`：`Std.Exception` →
`LockRecursionException` / `SynchronizationLockException`）。

**z42.core 里的 `[Native]` 包装同样不能改形状**：`ProcessNative` / `ThreadNative` 这类 core 静态类被别的包
（`z42.io` / `z42.threading`）调用，而 z42.core 总按当前源预建、别的包可能还是种子带来的旧版——同一个 libs
目录里新 core + 旧 io。改旧方法的返回类型，还在调它的旧包就会按旧形状读返回值（比如把 `void` 当数组）。做法：
core 里**加新方法**（`ThreadNative.Wait`、`ProcessNative.RunSync` / `Start`、`ChildProcessNative.*`、
`TcpSocketNative` / `UdpSocketNative` / `TlsSocketNative` / `DnsNative.Resolve`），新的 z42.io / z42.threading /
z42.net 改调新方法，旧方法与旧 builtin 一起挂债；种子不再声明旧名字后，core 侧旧声明与运行时旧表项、legacy
实现一并删除。

### 6.6 与 ext 表的关系

`[Native(lib="z42_compression", entry="__deflate_compress")]` 这类 facade 声明发的是同一条
`BuiltinInstr`——`lib=` 在这条路上**完全不参与解析**，只是给人看的标注。实际的库发现与符号注册在
[native-ext-loader.md](native-ext-loader.md)：ext builtin 的 id 在高位打标
（`BUILTIN_ID_EXT_BIT`），低 31 位是 ext 表下标；它们仍是旧 ABI（`&[Value]`），不参与折叠。

---

## 7. 内存管理

### 7.1 引用计数与 native 析构

描述符里的 `retain` / `release` 是为「native 实例纳入 z42 生命周期管理」准备的接口位：默认实现是对实例
第一个字段做原子增减，共享后端或池化场景可以自定义。

⚠️ **这套目前没有接线**。z42 侧持有的是一个 `long` 整数句柄，GC 不认识它，`dtor` / `dealloc` /
`retain` / `release` 四个回调在 VM 里没有调用点。释放由 z42 代码显式调 native 方法完成。要让它真正生效，
前提是先有 §8.5 的脚本可见类型形态——没有类型就没有"值被丢弃"这个事件可挂。

### 7.2 环收集

`Z42_TYPE_FLAG_TRACEABLE` 是给 native 类型加入环收集用的标志位，同样**尚未接线**。目标形态是由
`#[derive(Z42Type)]` 自动为字段生成遍历——相对 CPython 手写 `tp_traverse` 是严格改进（numpy 的大多数
引用计数 bug 出自那里）；但 derive 本身还是占位（§2.1）。

### 7.3 pin 协议

`PinPtr <local>` / `UnpinPtr <local>` 两条 opcode 表达"这段缓冲在 FFI 期间不可移动"。当前非移动堆下
`Str` 源是零成本（无处可移），`Array<u8>` 源退化为快照拷贝。移动 GC 落地后这里要改成向 pin set 登记
/ 注销，让压缩期跳过——接口位在 [object-abi.md §6](object-abi.md) 已留出。

---

## 8. 未落地的部分

### 8.1 Tier 3 源生成器

`CallNativeVtable` 在 `exec_native.rs` 里是**无条件 `bail!`**：

```rust
pub(super) fn call_native_vtable(vtable_slot: u16) -> Result<()> {
    bail!("CallNativeVtable not yet implemented (...): slot={vtable_slot}");
}
```

指令在 IR 与 zbc 里都有编号（见 [ir.md](../formats/ir.md)），编译器没有发射点。整条编译期绑定链
（读元数据 → 校验签名 → 发 vtable 槽 → 运行期解析为函数指针）都还不存在。

### 8.2 反向调用

`z42_invoke` / `z42_invoke_method` 在 `exports.rs` 里有 `#[no_mangle]` 入口但未接线，native 代码还不能
创建或调用 z42 对象。`Z42_VALUE_TAG_STR` / `OBJECT` / `TYPEREF` 三个 tag 的 marshal 路径同样空着——
tag 编号已冻结，占位在那里等接。

### 8.3 `pinned` 语法

`pinned` 在词法器里是关键字（`TokenKind.Pinned`），除此之外整条链都不存在：没有 AST 节点、没有语句
解析、没有类型检查、没有 IR 发射。`E0908a`（源类型不可 pin）/ `E0908b`（块内控制流跳出）两个诊断码有
定义、零发射点。§5 那两条 opcode 因此只有测试在驱动。

### 8.4 manifest 通路

`.z42abi` manifest 是 Tier 3 预留的元数据载体。仓里只有一个 JSON Schema 文件与一个校验它的测试
（`src/runtime/tests/manifest_schema_validation.rs`）——**没有生产者也没有消费者**：宏不产出 manifest，
编译器不读 manifest。相关诊断码 `E0909`（manifest 读取失败）/ `E0916`（native import 合成失败）有定义、
零发射点，见[诊断码全表](https://z42-lang.github.io/z42/reference/appendix/error-codes.html)。

### 8.5 native 类型怎样才能脚本可见

让 `import T from "lib";` 这类声明产出一个脚本可见类型，核心问题是**谁拥有实例内存布局**。四种形状：

| 形状 | 布局归属 | native 侧提供 | VM 侧做什么 |
|---|---|---|---|
| **A** native 拥有的 blittable | native `#[repr(C)]` struct | size / align / 每字段 offset + `alloc` / `dealloc` | 实例当不透明指针；描述符携带布局 |
| **B1** 句柄式 | native（不透明） | 只有函数回调；ctor 返回 `*mut Self`，方法首参收 `*mut/const Self` | 只存不透明句柄，无脚本可见字段 |
| **B2** VM 拥有字段 | VM | 函数回调 + 经新增的 `z42_obj_get/set_field` ABI 访问字段 | 分配带具名字段的对象，native 经回调看它们 |
| **C** 脚本侧 `[Repr(C)]` | 脚本 | — | 用户在 z42 里声明布局，编译器发匹配的描述符 |

**B1 是成本最低的起点**：95% 的 FFI 场景是包不透明句柄库（sqlite / curl / openssl / regex_t），B1 天然
贴合，且完全复用现有 ABI、不新增 VM 侧表面。B2 的收益（字段在调试器 / 反射里可见）不阻塞。C 让用户能在
不污染默认路径的前提下换到 A 式的直接字段访问。

今天连 B1 都没有：编译器里不存在任何从 native 元数据合成 `ClassDecl` 的 pass，用户只能自己写
`static extern` 方法并手工搬运 `long` 句柄。

---

## 9. 开放裁决

| # | 问题 | 倾向 |
|---|---|---|
| 1 | Tier 3 的元数据载体：JSON manifest / FlatBuffer / 直接复用 `.zpkg` 形态？ | 重启 Tier 3 时一并定；现有 JSON schema 不构成承诺 |
| 2 | 元数据分发位置：与 `.so` 同目录 / 嵌进 ELF section / 包注册表？ | 先同目录，注册表留到 1.0 后 |
| 3 | `import T from "lib"` 里的 `lib` 是库名（文件系统）还是包名（注册表）？ | 先库名 |
| 4 | Tier 1 ABI 的稳定性承诺：1.0 之后怎么破坏性变更？ | 等 Tier 1 有外部使用者再定 |
| 5 | 环收集：opt-in（`TRACEABLE` 标志）还是 native 类型默认开？ | opt-in（性能优先，也贴合 Rust 惯例） |

---

## 交叉引用

- 值表示与瞬态 arena 句柄模型：[object-abi.md](object-abi.md)
- IR 指令编号与 zbc 编码：[ir.md](../formats/ir.md)
- 宿主嵌入方向的同族 ABI：[embedding.md](embedding.md)
- cdylib 扩展范式与加载：[native-extensions.md](native-extensions.md) · [native-ext-loader.md](native-ext-loader.md) · [native-libraries.md](native-libraries.md)
- 用户侧契约：[Native 互操作（FFI）契约](https://z42-lang.github.io/z42/reference/embedding/native-interop.html)
