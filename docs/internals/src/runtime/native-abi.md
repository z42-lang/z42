# Native interop ABI（Tier 1 运行时）

> **相关**：[对象与值表示 ABI](object-abi.md) · [嵌入宿主](embedding.md) ·
> [Native 扩展库范式](native-extensions.md) ｜ **对齐**：2026-09-17
>
> 用户侧契约见参考手册 [Native 互操作（FFI）契约](../../../reference/src/embedding/native-interop.md)。
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
| [`src/runtime/include/z42_abi.h`](../../../../src/runtime/include/z42_abi.h) | Tier 1 C 头（与 `z42_host.h` 平行） |
| [`src/runtime/src/native/`](../../../../src/runtime/src/native) | Tier 1 在 VM 内的实现（`registry` / `marshal` / `dispatch` / `loader` / `error` / `exports` / `ext`） |
| [`src/runtime/crates/z42-abi`](../../../../src/runtime/crates/z42-abi) | Tier 1 的 Rust 镜像（结构体 + 冻结 tag 常量 + `extern "C"` 声明） |
| [`src/runtime/crates/z42-rs`](../../../../src/runtime/crates/z42-rs) | 面向用户的门面 crate（`Z42Type` trait + helper） |
| [`src/runtime/crates/z42-macros`](../../../../src/runtime/crates/z42-macros) | proc macro：`methods_attr` / `module_macro` / `shim` / `signature` |
| `src/compiler/z42c.semantics/src/StubEmitter.z42` | 编译器侧：`[Native]` → `CallNativeInstr` / `BuiltinInstr` 的分流 |

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
`z42.ir` 里连对应的指令类都没有。这两条 opcode 今天只由手写 IR 的测试驱动（§8.3）。

---

## 6. 内置函数通路（`BUILTINS`）

typed 通路之外的那一半：`[Native("__name")]` 把 stdlib 的 z42 声明接到 VM 里的 Rust 实现上。

### 6.1 表与 id

```rust
pub type NativeFn = fn(&VmContext, &[Value]) -> Result<Value>;

const PART1: &[(&str, NativeFn)] = &[ ("__println", io::builtin_println), … ];  // builtin_table.rs
const PART2: &[(&str, NativeFn)] = &[ … ];                                      // builtin_table_ext.rs
static JOINED: [(&str, NativeFn); TOTAL] = joined();   // const fn 编译期拼接
pub(crate) const BUILTINS: &[(&str, NativeFn)] = &JOINED;
```

**slice 下标就是 `BuiltinId`，而 `BuiltinId` 会被烤进 zbc**。因此：

- **只能表尾追加**。在中间插入会让所有既有产物里的 builtin 调用整体错位——这是本表最重要的一条不变式，
  表里多处 "appended to preserve existing BuiltinIds" 注释记的就是历次追加点。
- 名字 → id 的反查表 `BUILTIN_INDEX: OnceLock<HashMap<&str, u32>>` 首次访问时从 `BUILTINS` 现算
  （`corelib/mod.rs`），保证单一真相。
- 拆成 `PART1` / `PART2` 两段纯粹是行数门禁：表是按名字线性增长的数据，与 `mod.rs` 里的分发逻辑变更
  频率完全不同，合在一起会顶到 `xtask test lines` 的棘轮基线。

加载期由 `metadata::resolver` 把每个 builtin 调用点的名字解析成 token：先查 `BUILTINS`，未命中再查
per-VM 的 ext 表；两边都没有则留 `UNRESOLVED`，在真正调用时按名字再解一次
（JIT 在 ext 库尚未加载时就可能走到这里，硬 panic 会整个 VM 崩掉）。

### 6.2 命名约定

新 builtin 名一律 `__<area>_<verb>[_<modifier>]`，`<area>` 是单个词、内部不带下划线：

| area | 领域 | 例 |
|---|---|---|
| `str` | `Std.String` 上的字符串操作 | `__str_char_at` |
| `char` | `Std.Char` | `__char_to_upper`、`__char_is_whitespace` |
| `int32` / `int64` / `double` | 基元的 parse / hash / equals / to_string | `__int32_parse` |
| `math` | `Std.Math.Math` 静态方法 | `__math_sqrt`、`__math_atan2` |
| `obj` | 通用对象协议 | `__obj_get_type`、`__obj_hash_code` |
| `file` | `Std.IO.File` 静态方法 | `__file_read_text` |
| `env` | 环境 / 进程 | `__env_get`、`__env_args` |
| `time` | 时钟 / 计时 | `__time_now_ms` |
| `process` | 宿主进程控制 | `__process_exit` |

新领域 → 往这张表里加一行，再挑一个短单词。

**裸名一族**（`__println` / `__print` / `__readline` / `__concat` / `__contains` / `__len` /
`__to_str` / `__time_now_ms` / `__process_exit`）不遵守该约定，**不要再加新的**；它们所在的模块重组时
顺手迁走即可。

### 6.3 与 ext 表的关系

`[Native(lib="z42_compression", entry="__deflate_compress")]` 这类 facade 声明发的是同一条
`BuiltinInstr`——`lib=` 在这条路上**完全不参与解析**，只是给人看的标注。实际的库发现与符号注册在
[native-ext-loader.md](native-ext-loader.md)：ext builtin 的 id 在高位打标
（`BUILTIN_ID_EXT_BIT`），低 31 位是 ext 表下标。

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

`.z42abi` manifest 曾是 Tier 3 的元数据载体。今天仓里只剩一个 JSON Schema 文件与一个校验它的测试
（`src/runtime/tests/manifest_schema_validation.rs`）——**没有生产者也没有消费者**：宏不产出 manifest，
编译器不读 manifest。相关诊断码 `E0909`（manifest 读取失败）/ `E0916`（native import 合成失败）有定义、
零发射点，见[诊断码全表](../../../reference/src/appendix/error-codes.md)。

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
- 用户侧契约：[Native 互操作（FFI）契约](../../../reference/src/embedding/native-interop.md)
