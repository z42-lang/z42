# Native 互操作（FFI）契约

> **对齐**：2026-09-17 ｜ **状态**：Tier 1 C ABI 可用（仅解释器）；Tier 2 Rust 宏部分可用；Tier 3 未实现

本页是**扩展作者与宿主开发者**要查的契约：z42 代码怎么声明一个 native 调用、native 库怎么把类型注册进
VM、边界上允许出现哪些类型、谁负责释放什么。[嵌入概览](README.md) 讲的是反方向——宿主怎么启动并驱动
VM；两边共用同一套 `Z42Value` / `Z42Args` 值形状。

---

## 你要做的是哪件事

| 目标 | 写法 | 现状 |
|---|---|---|
| 调用 z42vm 内置的原生函数 | `extern` + `[Native("__name")]` | 名字必须**已在** VM 的内置表里；用户加不了新名字 |
| 调用随 SDK 分发的 native 扩展库（如压缩） | `[Native(lib="z42_x", entry="__y")]` | 可用；解析只看 `entry`，`lib` 是给人看的 |
| 调用**自己**用 C / Rust 写的 native 类型的方法 | `[Native(lib=, type=, entry=)]` + Tier 1 注册 | 可用，**仅解释器**；类型须由宿主或静态链接注册 |
| 在宿主程序里启动 z42 VM | `z42_host.h` | 见[嵌入概览](README.md) |

三条路共享同一条铁律：**跨边界的数据 100% blittable，没有自动编组**。高级类型（`string` / 数组 /
class 实例）不会被自动转换成 C 形状——要么投成标量后再传，要么根本不能传。

---

## 1. 契约要点

1. **零编组** —— 边界上只有 blittable 标量与指针。唯一的例外是 `string` → `*const c_char`，由 VM 在
   调用期间借出一份 NUL 结尾的副本（§5.3）。
2. **稳定 C 基座** —— 所有能力最终都落到 `z42_abi.h` 的 C ABI 上。Rust 宏只是它的糖；用 C / C++ / Zig /
   Go cgo 手写描述符能得到完全相同的效果。
3. **`abi_version` 永远是第一个字段** —— 新版本只在结构体尾部追加字段，不重排。
4. **panic 不过线** —— native 侧的 panic 必须在自己的 `extern "C"` 壳里截住；跨过 FFI 线是未定义行为。
5. **托管引用不出边界** —— GC 句柄（数组、class 实例、字符串对象本身）永远不会以原样交给 native 代码。

---

## 2. z42 侧：`extern` + `[Native]`

### 2.1 两种形态

```z42
namespace Demo;

public static class NumZ42 {
    // ① 带 type= —— Tier 1 typed native call，经 z42_register_type 注册的类型分派
    [Native(lib = "numz42", type = "Counter", entry = "__alloc__")]
    public static extern long CounterAlloc();

    [Native(lib = "numz42", type = "Counter", entry = "inc")]
    public static extern long CounterInc(long ptr);
}

public static class Console {
    // ② 位置串（或只有 entry=）—— 走 VM 内置表 / native 扩展表
    [Native("__println")]
    public static extern void WriteLine(string value);
}
```

判据只有一条：**`[Native]` 里有没有 `type=`**。

| 形态 | 分派到 | `lib=` 的作用 |
|---|---|---|
| `[Native("__name")]` / `[Native(entry="__name")]` | VM 内置函数表，未命中再查 native 扩展表 | — |
| `[Native(lib="L", entry="__name")]` | 同上（**按 `entry` 名解析，`lib` 不参与**） | 仅作文档说明 |
| `[Native(lib="L", type="T", entry="E")]` | Tier 1：`L::T::E`，经 `z42_register_type` 注册的方法表 | 参与解析，必须与注册时的 `module_name` 一致 |

第三种形态是扩展作者真正会用的：`lib` 对应描述符的 `module_name`，`type` 对应 `type_name`，`entry`
对应 `Z42MethodDesc.name`。三者任一对不上，调用在运行时报 `CallNative: unknown native type ...` /
`unknown method ...`。

### 2.2 方法必须怎么写

- 标 `extern`，**只有分号、没有方法体**。
- 参数与返回类型只能是 §5.1 的 blittable 集合。
- 实例方法的接收者以**第一个参数**出现在 ABI 签名里；上例把句柄显式写成 `long ptr`，是今天唯一可用的形状
  （native 类型还不能合成为脚本可见的 class，见 §5.5）。

> ⚠️ **编译期不校验这些规则**。`extern` 缺 `[Native]`、`[Native]` 用在非 `extern` 上、参数不 blittable
> ——这几条在诊断码表里都有编号（`E0903` / `E0904` / `E0907`），但**整组当前零发射点**，见
> [诊断码全表 E0901–E0916](../appendix/error-codes.md)。写错只会在运行时以 marshal 错误或
> `unknown native type` 的形式暴露。

---

## 3. Tier 1：C ABI

头文件：[`src/runtime/include/z42_abi.h`](../../../../src/runtime/include/z42_abi.h)。

### 3.1 类型描述符

```c
typedef struct Z42TypeDescriptor_v1 {
    uint32_t  abi_version;          /* 必须等于 Z42_ABI_VERSION（= 1） */
    uint32_t  flags;                /* Z42_TYPE_FLAG_* */
    const char* module_name;        /* 对应 [Native(lib=…)] */
    const char* type_name;          /* 对应 [Native(type=…)] */
    size_t    instance_size;
    size_t    instance_align;

    /* 生命周期回调 */
    void*   (*alloc)(void);
    void    (*ctor)(void* self, const Z42Args* args);
    void    (*dtor)(void* self);
    void    (*dealloc)(void* self);
    void    (*retain)(void* self);
    void    (*release)(void* self);

    size_t                method_count;
    const Z42MethodDesc*  methods;

    size_t                field_count;
    const Z42FieldDesc*   fields;

    size_t                trait_impl_count;
    const Z42TraitImpl*   trait_impls;
} Z42TypeDescriptor_v1;

typedef struct Z42MethodDesc {
    const char* name;       /* 对应 [Native(entry=…)] */
    const char* signature;  /* "(*mut Self, i64) -> i64" —— 语法见 §3.3 */
    void*       fn_ptr;     /* extern "C" 函数指针 */
    uint32_t    flags;      /* Z42_METHOD_FLAG_* */
    uint32_t    reserved;
} Z42MethodDesc;
```

`Z42FieldDesc`（`name` / `type_name` / `offset` / `flags`）与 `Z42TraitImpl`（`trait_name` +
`Z42MethodImpl[]`）同在头文件里。今天 VM 只消费 `methods`：**字段与 trait 实现会被解析但不参与派发**。

标志位：

| 组 | 取值 |
|---|---|
| `Z42_TYPE_FLAG_*` | `VALUE_TYPE` / `SEALED` / `ABSTRACT` / `TRACEABLE`（1 << 0..3） |
| `Z42_METHOD_FLAG_*` | `STATIC` / `VIRTUAL` / `OVERRIDE` / `CTOR`（1 << 0..3） |
| `Z42_FIELD_FLAG_*` | `READONLY` / `INTERNAL`（1 << 0..1） |

### 3.2 值、参数与错误

```c
typedef struct Z42Value {
    uint32_t tag;       /* Z42_VALUE_TAG_* */
    uint32_t reserved;
    uint64_t payload;   /* 整数 / f64 位型 / 指针 */
} Z42Value;

typedef struct Z42Args { size_t count; const Z42Value* items; } Z42Args;
typedef struct Z42Error { uint32_t code; const char* message; } Z42Error;
```

tag 值已冻结，只能追加：`NULL=0` `I64=1` `F64=2` `BOOL=3` `STR=4` `OBJECT=5` `TYPEREF=6`
`NATIVEPTR=7` `PINNED_VIEW=8`。

> `Z42Value` / `Z42Args` 与[嵌入 API](README.md) 的 `z42_host.h` **是同一份定义**，两套头文件并行但不
> 重复声明。`Z42Error.message` 由 VM 拥有，**不要 free**。

### 3.3 签名串语法

`Z42MethodDesc.signature` 是 `(P1, P2, …) -> R` 形式的字符串，VM 在注册时解析它并预建 libffi 调用接口。
每个位置只接受下面这些拼法，其余一律注册失败：

| 拼法 | 含义 |
|---|---|
| `()` / `void` | 无返回值 |
| `i8` `i16` `i32` `i64` `isize` | 有符号整数（`isize` 同 `i64`） |
| `u8` `u16` `u32` `u64` `usize` | 无符号整数（`usize` 同 `u64`） |
| `f32` `f64` | 浮点 |
| `bool` | 1 字节 |
| `CStr` | NUL 结尾的借用 C 字符串（线型即指针） |
| `Self` `&Self` `&mut Self` `*const Self` `*mut Self` | 接收者；线型即指针 |
| `*const T` / `*mut T`（任意 `T`） | 裸指针；**元素类型在 ABI 层被抹掉** |

### 3.4 VM 暴露的 C 函数

```c
Z42TypeRef z42_register_type(const Z42TypeDescriptor_v1* desc);
Z42TypeRef z42_resolve_type(const char* module, const char* type_name);
Z42Value   z42_invoke(Z42TypeRef ty, const char* method, const Z42Value* args, size_t n);
Z42Value   z42_invoke_method(Z42Value receiver, const char* method, const Z42Value* args, size_t n);
Z42Error   z42_last_error(void);
```

`z42_register_type` / `z42_resolve_type` / `z42_last_error` 可用。**`z42_invoke` /
`z42_invoke_method` 尚未接线**——native 侧还不能反过来构造或调用 z42 对象（§6.2）。

`z42_register_type` 返回 `NULL` 即失败，原因经 `z42_last_error()` 取。注册会校验 `abi_version`、
读出全部方法描述符并解析签名串，因此**签名写错在注册时就失败**，不会拖到调用时。

### 3.5 native 库怎么被装进来

注册入口按文件名约定：库文件 `libnumz42.dylib` ⇒ VM 查找并调用符号 **`numz42_register`**
（`void(void)`），由它在内部对每个类型调 `z42_register_type`。

⚠️ **VM 没有任何自动 `dlopen` 这类库的路径**。今天让 Tier 1 类型进到 VM 只有两条：

- 把 native 库**静态链接**进宿主，并在跑 z42 代码前显式调用它的注册入口；
- 宿主经嵌入侧的 API 显式加载。

（随 SDK 分发、启动时被自动扫描加载的是**另一条路**——不带 `type=` 的扩展库形态，见 §2.1。）

### 3.6 ABI 演进规则

- `abi_version` 永远在偏移 0；新版本**只追加**字段，不重排、不改语义。
- VM 按 `abi_version` 决定读多大，不假设布局。
- 一切访问经 `z42_*` 函数，不要直接改 VM 交回的结构体。
- 主版本号变更 = 显式 break（记在 semver-major 里）。

### 3.7 谁该直接用 Tier 1

- 给既有 C 库（sqlite3、ffmpeg、openssl）写绑定；
- 从非 Rust 宿主（C / C++ / Zig / Go cgo）暴露类型给 z42；
- 需要精确控制 `retain` / `release` 或实例分配策略的场合。

---

## 4. Tier 2：Rust 侧的宏

三个 crate 在 [`src/runtime/crates/`](../../../../src/runtime/crates)：

| crate | 职责 |
|---|---|
| `z42-abi` | Tier 1 的 Rust 镜像（结构体 + tag 常量 + `extern "C"` 声明） |
| `z42-rs` | 面向用户的门面（`Z42Type` trait、helper） |
| `z42-macros` | proc macro：`#[z42::methods]`、`z42::module!` |

### 4.1 可用的写法

```rust
pub struct Counter { value: i64 }

#[z42::methods(module = "numz42_rs", name = "Counter")]
impl Counter {
    pub fn inc(&mut self) -> i64 { self.value += 1; self.value }
    pub fn get(&self)     -> i64 { self.value }
}

z42::module! {
    name:  "numz42_rs",
    types: [Counter],
}
```

- `#[z42::methods]` 的两个键 **`module` 与 `name` 都必填**；impl 目标必须是**无泛型的单个类型名**。
- 宏为每个方法生成 `extern "C"` 壳 + `Z42MethodDesc` 条目，并合成 `alloc` / `dealloc` / `ctor` / `dtor`
  生命周期回调，拼出一个 `static Z42TypeDescriptor_v1`。
- `z42::module!` 只接受 `name:` 与 `types:` 两个字段，展开成 **`#[no_mangle] extern "C" fn <name>_register()`**
  ——即 §3.5 要的那个入口。它**不会**自动在库加载时运行，也**不产出任何 manifest 文件**。

### 4.2 Rust 类型 ↔ ABI

宏只接受下面这些；其余（`&T`、`&str`、`String`、`Vec<T>`、`&[T]`、`Box<T>`、`Result<T, E>`、
按值 `self`、复杂路径类型）会给出编译错误。

| Rust | ABI | 位置 |
|---|---|---|
| `i8`/`i16`/`i32`/`i64`/`isize` | 同名整数（`isize`→`i64`） | 参数 + 返回 |
| `u8`/`u16`/`u32`/`u64`/`usize` | 同名整数（`usize`→`u64`） | 参数 + 返回 |
| `f32` / `f64` / `bool` | 同名 | 参数 + 返回 |
| `&self` / `&mut self` | `*mut Self` | 接收者 |
| `Self` / `*const Self` / `*mut Self` / `&Self` / `&mut Self` | `*mut Self` | 参数 + 返回 |
| `*const T` / `*mut T` | `*mut void` | 参数 + 返回 |
| `()` / 无返回 | `()` | 返回 |

### 4.3 未实现

`#[derive(Z42Type)]` 与 `#[z42::trait_impl(...)]` **当前是占位**：用了它们会直接得到一条
`compile_error!`。这意味着 Tier 2 今天**不能**导出字段、也不能让 native 类型实现 z42 接口——
`z42::module!` 展开时要求目标类型实现 `Z42Type`，所以实际使用中描述符仍需手写或由 `#[z42::methods]`
配套提供。

---

## 5. 边界上的类型

### 5.1 允许出现的类型

| z42 侧值 | ABI 目标位置 | 说明 |
|---|---|---|
| `int` / `long`（`Value::I64`） | `i8`…`i64` / `u8`…`u64` | 只读低 N 字节，宽度由签名定 |
| `int` / `long` | `*const T` / `*mut T` / `Self` | **裸指针以整数承载**（句柄模式，见 §5.4） |
| `bool` | `bool` | 1 字节 |
| `double`（`Value::F64`） | `f64` / `f32` | 传 `f32` 位由 libffi 收窄 |
| `null` | `*const T` / `*mut T` / `Self` / `CStr` | 空指针 |
| `string` | `CStr` / `*const T` | 见 §5.3 |

返回方向：`NULL`→`null`，`I64`→整数，`F64`→浮点，`BOOL`→布尔，`NATIVEPTR`→整数句柄。
`STR` / `OBJECT` / `TYPEREF` 三个 tag 虽已冻结编号，**返回路径尚未接线**，收到会报错。

### 5.2 不允许出现的类型

数组、`List<T>`、class 实例、struct、`char`、tuple、委托、闭包——一律不能出现在 native 调用的参数或
返回位置。越界不会在编译期被拦下，而是在调用时报
`marshal: cannot pass z42 … as native arg of type …`。

也没有 `[Layout]` / `[FieldOffset]` 这类布局控制属性：z42 的 struct **无法声明 C 布局**，因此 struct 不能
按值跨边界。要传结构化数据，只能自己在 native 侧分配、把指针当 `long` 句柄传回来。

### 5.3 `string` → `*const c_char`

传字符串给 libc 风格的函数不需要任何特殊语法：

```z42
[Native(lib = "mylib", type = "Log", entry = "write")]
public static extern int Write(string message);
```

- VM 在调用期间分配一份 **NUL 结尾**的副本，调用返回后立即释放。
- native 侧只在**这次调用期间**持有该指针有效，**不得保存**。
- 字符串内部含 `\0` 时抛 `Std.InvalidMarshalException`（C 侧无法区分它与结束符），脚本可捕获。

### 5.4 句柄模式

native 实例今天以**不透明整数句柄**的形式活在 z42 里：分配方法返回 `long`，后续方法把它当第一个参数传
回去。

```z42
long ptr = NumZ42.CounterAlloc();
NumZ42.CounterInc(ptr);
long n = NumZ42.CounterGet(ptr);
```

句柄的生命周期**完全由你负责**：z42 侧的 `long` 只是一个数字，GC 不认识它，也不会替你调 `dtor` /
`dealloc`。描述符里的 `retain` / `release` 目前不参与 z42 的引用计数。

### 5.5 尚未提供的形态

| 形态 | 现状 |
|---|---|
| `pinned p = s { … }` 零拷贝借用块 | `pinned` 是**保留关键字**，但没有语法支持；编译器无法解析该语句 |
| `import T from "lib";` 自动生成绑定 | 未实现；`.z42abi` manifest 机制不存在 |
| `extern class T { … }` 声明 native 类 | 未实现 |
| native 类型呈现为脚本可见的 class（字段访问、`is` 判定、接口实现） | 未实现；见 §5.4 的句柄模式 |
| `[UnmanagedCallback]` 把 z42 函数交给 native 作回调 | 未实现 |

---

## 6. 调用约定与错误

### 6.1 z42 → native

调用形状是平台 C 调用约定：`fn(self_ptr?, arg1, …, argN) -> ret`。没有隐式上下文参数（没有 GIL 句柄、
没有线程上下文），也没有编组桩——只有一次间接调用。

**执行引擎限制**：native 调用只在**解释器**上工作。含 native 调用的方法不会被 JIT 编译，会整体回落解释
执行；AOT 同样不覆盖。这不影响正确性，但热路径上的 native 调用得不到 JIT 收益。

### 6.2 native → z42

`z42_invoke` / `z42_invoke_method` **尚未接线**，native 代码目前无法反过来创建或调用 z42 对象。
需要回调的场景只能由宿主在 z42 之外自行编排。

### 6.3 错误

| 情况 | 表现 |
|---|---|
| 签名串非法 / `abi_version` 不符 | `z42_register_type` 返回 `NULL`，原因经 `z42_last_error()` |
| `lib` / `type` / `entry` 对不上已注册项 | 运行时错误 `CallNative: unknown native type …` / `unknown method …` |
| 实参个数与签名不符 | 运行时错误 `arity mismatch` |
| 值不在 §5.1 集合内 | 运行时错误 `marshal: cannot pass …` |
| 字符串含内部 NUL | 抛 `Std.InvalidMarshalException`（可 `catch`） |
| native 侧 panic / 崩溃 | **未定义**——必须在自己的 `extern "C"` 壳里截住 |

---

## 7. 跨边界生命周期规则

- **z42 传出去的指针只在这次调用期间有效**（`CStr` 副本、任何由 VM 借出的缓冲）。native 侧**不得保存**，
  调用返回后即失效。
- **native 分配的对象由 native 负责释放**。把它当 `long` 句柄交给 z42 不会让 GC 接管；配对的释放调用要由
  z42 代码显式发起。
- **托管对象永远不跨线**：数组、class 实例、字符串对象本身不会以 GC 句柄形式交给 native 代码。
- **不要跨线程持有句柄**而不加同步：VM 侧的注册表是全进程共享的，但 native 对象的线程安全由你自己保证。

---

## 相关

- [嵌入概览](README.md) —— 宿主怎么启动并驱动 VM
- [诊断码全表](../appendix/error-codes.md) —— `E0901`–`E0916` 原生互操作码组
- [参数修饰符 `ref` / `out` / `in`](../language/parameter-modifiers.md) —— 这三个修饰符**不参与** native 签名
