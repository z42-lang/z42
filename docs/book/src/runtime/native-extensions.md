# Native 扩展库（独立 cdylib 机制）

z42vm 把一部分「重、依赖多、或平台特有」的能力放进**独立编译的 native 动态库**（cdylib），
运行期经 `dlopen` 加载，用一套**纯 C ABI** 与核心 z42vm 交互。这样做的收益：

- **依赖隔离**：库自己的 Rust 依赖树（rustyline 的 ~19 个 crate、zstd/brotli/lz4 的 C 绑定）
  不进核心 z42vm crate，核心的编译时间 / 二进制体积 / 供应链面都更小。
- **平台裁剪**：wasm / 移动端可以整库不带（用 fallback），或改成静态链接（`bundled-*` feature）。
- **通用范式**：新增一个 native 能力 = 新建一个 crate + 加一段加载臂，不动核心执行引擎。

目前有两个实例，分处这套机制的两个极端，合起来定义了完整的设计空间：

| 维度 | `z42-compression`（stdlib ext） | `z42-repl`（host-only toolchain） |
|------|-------------------------------|----------------------------------|
| 加载时机 | **急切**：VM 启动扫描 `<sdk>/native/` | **懒**：首次 `__repl_readline` 才 `dlopen` |
| 方向 | **单向** VM → native（纯计算） | **双向**：native 还要回调进 VM（补全 / 键位） |
| 打包位置 | `<sdk>/native/`（跨平台 stdlib 扩展） | `<sdk>/programs/z42i/`（平铺在 z42i payload 旁，host-only 组件私有） |
| 发现路径 | `ext::native_search_paths()` | repl 专用 `repl_native::candidates()` |
| 缺库行为 | 对应 stdlib facade 抛 NotSupported | 退回 plain-stdin 逐行读 |
| 边界数据 | `*const u8` 字节缓冲 | C 字符串（prompt / 候选 / 动作串） |

> 铁律（两个实例都遵守）：**z42 内部类型（`Value` / `VmContext`）绝不跨 C 边界。**
> 一切进出都 marshal 成 C 基本类型（`*const u8` / `*const c_char` / `usize` / `i32`）；
> `VmContext` 只以 **opaque `*mut c_void`** 透传，且**只在 z42vm 侧的 trampoline 内**才 cast 回
> `&VmContext`。cdylib 永不 deref 它。这让 cdylib 对 z42 主 crate **零依赖**（无 Cargo 环）。

---

## 1. 单向范式：`z42-compression`

最简单的形状——VM 调 native，native 算完把结果字节缓冲还回来。

- **crate**：`src/runtime/crates/z42-compression`（cdylib+rlib）。导出 `#[unsafe(no_mangle)]`
  的 `z42_compression_*`：入 `(*const u8, len, params…)`，出参 `(*mut *mut u8, *mut usize)` +
  `i32` 返回码；错误经 thread-local `z42_compression_last_error()` 取串；缓冲用**库自己的
  分配器**分配、经 `z42_compression_free(ptr, len)` 释放。
- **加载**：`src/runtime/src/native/ext.rs` 在 VM 启动 `load_all` 里扫 `native_search_paths()`
  下的 `libz42_*.{so,dylib,dll}`，`match name { "compression" => … }` 解析已知符号集，把每个
  `__<entry>` builtin 名注册进 `VmCore.ext_builtins`（供 interp/JIT dispatch）。
- **wrap shim 留在 z42vm 侧**：每个 builtin 有一个 `wrap_*(ctx, args) -> Result<Value>`，负责
  `Value` ↔ `*const u8` 的 marshal，再调库函数指针。marshal 逻辑属于 z42vm，不进库。
- **分配器纪律**（unify-gc-heap PR-3 教训）：库返回的缓冲**必须用库自己的 `free`** 释放
  （`take_owned_buffer` 先 copy 进 z42vm 的 Vec、再调库 free）——`dlopen` 的库可能与 z42vm 用
  不同 global allocator，混用会 Linux 段错误、macOS 侥幸不炸。
- **wasm**：`bundled-compression` feature 把 rlib 静态链进来，`load_all` 短路到直接注册函数
  指针，`ext_builtins` 表填法不变，消费方无感。

搜索路径（`native_search_paths()`，急切扫描用）按序：`Z42_NATIVE_PATH` 覆盖 → `<exe>/../native/`
（SDK 布局）→ `<exe>/native/` → `<exe>/`（dev cargo target 直放）。**都必须先 `sort` 再迭代**
（common-pitfalls §1：`read_dir` 顺序依赖 OS/FS，first-wins 注册会非确定）。

---

## 2. 双向范式：`z42-repl`

REPL 行编辑器（rustyline 后端）比 compression 多两个难点：**要回调进 VM**（Tab 补全查会话符号、
缩进键位问 z42 决策），且**是 host-only 工具链件**（不跨平台、不进 `native/`）。

### 2.1 crate 与 C ABI

`src/runtime/crates/z42-repl`（cdylib+rlib，只依赖 rustyline）导出三个入口：

```
z42_repl_readline(prompt: *const c_char,
                  cbs: *const ReplCallbacks,
                  out_kind: *mut i32) -> *mut c_char   // 返回一行；owned C 串
z42_repl_free(*mut c_char)                              // 释放上面的返回串
z42_repl_last_error() -> *const c_char                  // Z42_REPL_ERROR 时取消息
```

`out_kind` 是这套 ABI 的关键——它把 rustyline 的多种结局编码成小整数，让 z42vm 侧决定语义映射：

| out_kind | 含义 | z42vm 映射 |
|----------|------|-----------|
| `0` LINE | 读到一行/一段 | `Value::Str(line)` |
| `1` EOF | Ctrl-D | `Value::Null`（→ z42 循环退出） |
| `2` INTERRUPT | Ctrl-C | `Value::Str("")`（放弃当前 buffer、重提示） |
| `3` ERROR | 真错误 | `bail!`（消息取自 `last_error`） |
| `4` NO_EDITOR | 编辑器起不来（无 tty） | 退回 `plain_readline` 逐行读 |

> `ERROR` vs `NO_EDITOR` 分开是有意的：旧的 in-VM 实现里，`Editor::with_config` 失败（无 tty）
> 会**静默降级**到 plain 逐行读，而真正的读取错误才报错。若只有一个 ERROR 码，z42vm 无法区分
> 「该兜底」和「该报错」。

### 2.2 回调表 `ReplCallbacks`

VM → native 是 `dlopen` 符号；native → VM 是**函数指针**。z42vm 每次 `readline` 前构造一张
`#[repr(C)]` 的回调表交给库：

```rust
#[repr(C)] struct ReplCallbacks {
    ctx: *mut c_void,   // opaque *mut VmContext，库只透传不 deref
    complete:  extern "C" fn(*mut c_void, *const c_char, usize) -> *mut c_char, // (ctx,line,pos)→候选
    key_edit:  extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> *mut c_char, // (ctx,key,line,pos)→动作串
    free_str:  extern "C" fn(*mut c_char),  // 释放上两者返回的串
}
```

库的 rustyline `Completer` / 键位 handler 在 `readline` 过程中同步调用这些指针；`complete` 返回
`\n`-join 的候选串（库自行 split），`key_edit` 返回动作串（`""`=默认键为、`dedent`/`insert:…`/
`newline:…`/`accept`）。**这张表必须与 z42vm 侧手抄的 `ReplCallbacks` 布局逐字节一致**——
两侧从同一源码树版本锁一起构建，不匹配在打包链接期就暴露，不会静默到运行期。

### 2.3 trampoline：唯一 cast 回 `&VmContext` 的地方

z42vm 侧为 `complete` / `key_edit` 各提供一个 `extern "C"` trampoline
（`corelib::repl::complete_trampoline` / `corelib::repl_editing::keyedit_trampoline`）。它们是
**边界铁律的守门人**：

```
extern "C" fn complete_trampoline(ctx: *mut c_void, line: *const c_char, pos) -> *mut c_char {
    if ctx.is_null() || line.is_null() { return null }       // 防御
    let fqn = REGISTERED_COMPLETER.get()? else { return null } // 无补全器→默认
    let vmctx = &*(ctx as *const VmContext)                    // ← 唯一 cast 点
    let _unpark = NativeUnparkGuard::exit(vmctx)               // GC：见 2.5
    match complete_via_callback(vmctx, &fqn, Str(line), pos) { // 重入 VM 跑 z42 补全器
        Ok(Array(cands)) => CString::new(cands.join("\n")).into_raw(),
        _ => null,   // throw 被吞成 no-op（补全器抛异常不该崩 REPL）
    }
}
```

`ctx` 指针的有效性靠**同步 + 同线程**保证：`builtin_repl_readline` 持 `&VmContext` 活过整个
`(lib.readline)(…)` 调用，库只在该调用内、同一线程上同步回调，返回后指针即弃。等价于旧
实现里 `readline` 期间发布的 `ACTIVE_CTX` thread-local，只是现在改为经参数透传。

### 2.4 懒加载与发现（`corelib::repl_native`）

与 compression 的急切扫描相反，repl 库**首次 `__repl_readline` 才 `dlopen`**（`OnceLock` 记忆
结果，探一次文件系统），启动路径完全不碰它。发现顺序（**不走** `native_search_paths()`）：

1. `Z42_REPL_NATIVE`（env 覆盖，接受库文件全路径或含库的目录）；
2. `current_exe().parent()/libz42_repl.{so,dylib}`（**dev** cargo target 目录，
   `cargo build -p z42-repl` 落在 z42vm 旁）；
3. 从运行 `<sdk>/bin/<app>` 派生 `<sdk>/programs/z42i/`，经共享的 `ext::resolve_native_beside`
   解析（**SDK** 布局：repl 库是「组件私有 native」，平铺在 interactive payload 旁，**不在** `bin/`
   ——见 [Native 库的布局与解析](native-libraries.md)）。

找不到 / 加载失败 / `native-interop` feature 未开 / wasm → 一律退回 `plain_readline`。整个
`repl_native` 模块 gated `not(target_arch = "wasm32")`，内部 dl 逻辑再 gated `feature = "native-interop"`。

### 2.5 GC 重入协议

`builtin_repl_readline` 阻塞读 stdin 前先 `NativeParkGuard::enter`——把本线程**停靠**，让后台
prewarm 线程的 GC 能在本线程等输入时推进。但回调（`complete`/`key_edit`）会在 `readline` 中途
重入 VM 跑 z42 代码，此时必须**临时 unpark**（trampoline 内 `NativeUnparkGuard::exit`），让回调
作为正常 mutator 在自己的 safepoint 停靠；回调返回、`_unpark` drop 后自动重新停靠回阻塞读。

### 2.6 打包（host-only 组件私有，进 programs/z42i/ 不进 bin/ / native/）

SDK 打包（`scripts/package/`）：`libz42_repl` 由 **z42.repl 的 build hook**（`ProvideNative`）在
`z42b publish z42.interactive` 时 `cargo build -p z42-repl` 产出，经 `_pubBundleProjectNativeDeps`
平铺进 **z42i 组件的 `programs/z42i/`**（`[assemble]` 自动并入 `pkgDir`，与 2.4 的 `<sdk>/programs/z42i/`
发现对齐）——**不再有** xtask `_pkgStageReplCdylib` 特殊处理（add-native-dep-config）。两条**关键排除**：
① `_pkgStageZ42vm` **不**往 `bin/` 放 repl 库；② `_copyNativeLibs` 的 `libz42*` glob **显式排除**
`libz42_repl`/`z42_repl`（hook 把它建进共享 cargoOut），否则会误拷进
`<sdk>/native/`。把 repl 移出共享 `bin/` 是为根治 §1 急切扫描器对它喷 `ignoring unknown lib repl`
——它是**组件私有 native**（跟随 z42i），不是 `<sdk>/native/` 里的跨平台 stdlib 扩展，也不是
`bin/` 的通用可执行件。布局/解析全轴见 [Native 库的布局与解析](native-libraries.md)。dev 流不建
cdylib（与 compression 一致，靠开发者 `cargo build -p`）。

---

## 3. 新增一个 native 扩展的清单

1. 建 crate `src/runtime/crates/z42-<name>`（cdylib+rlib，对 z42 主 crate 零依赖）；导出
   `#[unsafe(no_mangle)] z42_<name>_*` 的纯 C ABI（错误经 thread-local last-error + 返回码）。
2. 决定形状：**单向**（照 compression：进 `native/`、急切扫描、wrap shim + `ext_builtins`）还是
   **双向 / host-only**（照 repl：回调表 + trampoline + 懒 dlopen + 专用发现路径 + 进 bin/）。
3. z42vm 侧加加载臂：单向在 `native/ext.rs` 的 `match name`；双向另起一个 `corelib::<name>_native`
   懒加载模块。**符号名两侧手抄，务必逐字节对齐**（版本锁同树构建 → 打包期暴露不匹配）。
4. 打包接线（`scripts/package/`）：`cargo build -p z42-<name>` + 拷到正确位置——跨平台 stdlib 扩展
   进 `<sdk>/native/`；**组件私有 native**（host-only / 跟随某组件）平铺进该组件 payload 旁
   （如 repl 的 `programs/z42i/`，见 [Native 库的布局与解析](native-libraries.md)）。后者记得在
   `_copyNativeLibs` 里把它从 `<sdk>/native/` glob 排除。
5. 缺库 fallback：想清楚库不存在时的降级行为（抛 NotSupported / plain 兜底 / bundled 静态链）。
6. **边界铁律自查**：有没有让任何 z42 类型跨了 C 边界？`VmContext` 只 opaque 透传、只在 trampoline
   cast 回？回调返回的串谁分配、谁 free（用哪侧的 `free_str`）？

> 相关：[Native 库的布局与解析](native-libraries.md)（库住哪 / 怎么找 / 发布期拍平）、
> [加载上下文（LoadContext）](load-context.md)、[GC 调参与 safepoint 协议](gc-tuning-and-safepoint.md)、
> [REPL 输入完整性判定](../toolchain/repl-input-completeness.md)。
