# Design: 独立 native 库机制 + REPL cdylib 剥离

## §0 现有三套 native 机制（先厘清，避免混淆）

z42vm 现在有**三条**不同的 native 路径，本 change 只动第 2 条并把它通用化：

| # | 机制 | 用途 | 值方向 | 定位 | 代码 |
|---|------|------|--------|------|------|
| 1 | **静态 builtin** | `__str_*` / `__repl_*` 等内建 | VM→fn，`&VmContext` + `&[Value]` | 编译进 z42vm | `corelib/*.rs` 派发表 |
| 2 | **ext cdylib** | compression（`__deflate_*`）| VM→cdylib，纯 C ABI 值 in/out | dlopen `<sdk>/native/libz42_*` | `native/ext.rs` |
| 3 | **Tier-1 C ABI** | 用户 `[Native]`/`extern class` | libffi 通用派发（blittable）| dlopen + descriptor | `native/{registry,dispatch,marshal}.rs` |

**本 change = 把 REPL 从机制 1 迁到机制 2，并把机制 2 从「compression 硬编码」通用化成可复用范式，外加它
现在缺的两件事：懒加载 + re-entrancy 回调。** 机制 3（libffi 通用派发）是用户面 interop，本 change 不动。

## §1 通用「独立 native 库」生命周期

一个独立 native 库（`z42-<name>`）走三段：

```
编译 (compile)                链接/定位 (link/locate)          运行/分发 (run/dispatch)
──────────────                ─────────────────────           ──────────────────────
crates/z42-<name>/     ──►    产物落 <sdk>/native/       ──►   z42vm 注册 <name> 的
  crate-type =                  libz42_<name>.{so,             builtin 名 → 薄 shim；
  [cdylib,staticlib,rlib]        dylib,dll}（desktop）          首次调用懒 dlopen；
  纯 C ABI 导出 +                staticlib/rlib（wasm/          shim marshal Value↔C
  _register 清单                mobile，bundled feature）        并调 dlopen'd C 符号
  无 z42 主 crate 依赖          发现：Z42_NATIVE_PATH /          （需回调则传 C 回调
  （无 Cargo 环）               <exec>/../native/ 等            指针 + opaque ctx）
```

### §1.1 编译（compile）

- 每个 native 库是 `src/runtime/crates/z42-<name>/` 下的独立 cargo crate，
  `crate-type = ["cdylib", "staticlib", "rlib"]`（同 compression）。
- **铁律：cdylib 不依赖 z42 主 crate**（无 Cargo 循环）。跨边界只走 C ABI（`*const u8`/`usize`/`i32`/
  `*const c_char`/函数指针/`*mut c_void`）——**z42 内部类型（`Value`/`VmContext`）绝不过边界**。
- cdylib 导出 `#[unsafe(no_mangle)] pub extern "C"` 函数 + 一个 `z42_<name>_register()` 返回
  `(name, fn_ptr)` 清单（z42vm 读它填 `ext_builtins`）。

### §1.2 链接/定位（link/locate）

native 库分**两类归属**（本 change 确立此区分，User 2026-08-29 定）：

| 类别 | 例 | 归属 | 打包位置 | 平台 | 发现 |
|------|----|------|---------|------|------|
| **跨平台运行时/stdlib 扩展** | compression | runtime（`src/runtime/`）| `<sdk>/native/` | 全平台（wasm bundled）| `native_search_paths()` |
| **host-only 工具链扩展** | **repl** | **toolchain**（随 z42i）| **z42i 同侧的 toolchain 子目录**（非 `<sdk>/native/`）| **仅 host**（wasm/mobile 无，走各自 fallback）| **z42i-adjacent 专用路径**（见 §4）|

- **z42-repl 是 host-only 工具链件**，不是跨平台 runtime 的一部分：它只被 z42i（REPL apphost）用，只在
  desktop host 存在，**不进 `<sdk>/native/`**（那是给「任意 z42 程序在任意平台」用的 stdlib 扩展），而是
  打包到 **z42i 同侧的 toolchain 子目录**（与 Task 2「repl 折进 interactive」一致——最终落 interactive 侧）。
- **wasm/mobile 无 repl cdylib**：REPL 本就 host-only；wasm 保留 plain-stdin fallback（cfg-gate，无 rustyline），
  mobile 无交互 REPL。故**不需要** `bundled-repl` / staticlib（与 compression 的全平台策略不同）。
- **crate 源位置**：`src/runtime/crates/z42-repl/`（cargo workspace 成员——Rust crate 的构建现实；与
  compression 同处 `crates/`）。「toolchain 归属」由**打包位置 + host-only gating + 文档**表达，非源码物理位置。
  （把源搬出 cargo workspace 会显著复杂化构建，收益低，不做。）

### §1.3 运行/分发（run/dispatch）

- z42vm 侧对每个已知 native 库有一段**注册逻辑**（`native/ext.rs::load_one` 的 `match name`），把该库的
  C 符号包成 `NativeFn`（`fn(&VmContext, &[Value]) -> Result<Value>`）的 **wrap shim**，注册进
  `ext_builtins`（builtin 名 → shim，带高位 `0x8000_0000` 与静态 builtin 区分）。
- z42 用户/stdlib 代码用 `[Native("<name>")]` 声明，调用时经 `ext_builtin_id_of` → `ext_builtins.dispatch`
  路由到 shim。shim 做 `Value`↔C marshal、调 dlopen'd 符号、把 C 错误码翻成 z42 异常。

> **为什么 shim 留在 z42vm 而非 cdylib**：shim 需要 `&VmContext`（建 `Value`、访问 GC 堆、抛异常），这些是
> z42 内部类型，不能过 C 边界。cdylib 只做「纯计算 / 纯 IO」，marshal 与 VM 交互留 z42vm。这条边界是整个
> 机制的**关键不变量**。

## §2 单向 C ABI 约定（值 in/out，compression 范式）

无需回调的库（compression、未来的 hashing/图像编解码等）：

```
z42vm shim (has &VmContext)          cdylib (pure C ABI, no VM types)
  Value::Array<I64> ──marshal──►     z42_<name>_op(in_ptr, in_len, ...,
                                        out_ptr, out_len) -> i32(errcode)
  ◄──bytes_to_value── out buffer       z42_<name>_last_error() -> *const c_char
                                       z42_<name>_free(ptr, len)
```

- 错误：返回码 + thread-local last-error 串（shim 读它翻成 z42 异常）。
- 内存：cdylib 分配的 buffer 由 cdylib 的 `_free` 释放（跨-allocator 安全）。
- 错误码数值是 C ABI 一部分，稳定；加可以，改/删是 major bump。

## §3 双向 re-entrancy 回调 ABI（native 回调 z42——REPL 必须，compression 没有）

REPL 的难点：rustyline 的 `readline()` 是**阻塞调用**，执行期会触发**补全/键位回调**，这些回调要**回调
z42 函数**（用户注册的 `replComplete`/`replKeyEdit`）。回调需 `&VmContext`（`exec_function` 重入）——
不能过 C 边界。**解法：回调实现留 z42vm，cdylib 只持有指向它们的 C 函数指针。**

```
z42vm shim                                cdylib z42_repl_readline(prompt, cbs, ctx)
──────────                                ──────────────────────────────────────────
__repl_readline(&VmContext):              装配 rustyline Editor，绑定 Completer /
  懒 dlopen libz42_repl                      KeyEditHandler（rustyline trait 实现）；
  cbs = ReplCallbacks {                     ed.readline(prompt) 阻塞——
    complete: z42vm_complete_trampoline,      需补全时 → cbs.complete(ctx, line, pos)
    key_edit: z42vm_keyedit_trampoline,       需键位时 → cbs.key_edit(ctx, key, line, pos)
  }                                         返回 *mut c_char（行）或 null（EOF）
  ctx = self as *mut c_void  ───────────►
  line = z42_repl_readline(prompt,cbs,ctx)
       │
       └─ trampoline(ctx, line, pos):       // 在 z42vm 侧执行，有 VmContext
            let vm = &*(ctx as *VmContext)
            NativeUnparkGuard::exit(vm)      // GC：native 阻塞期 park，回调期临时 unpark
            exec_function(vm, replComplete…) // 重入 VM 跑 z42 补全器
            → 结果 marshal 回 C 字符串数组
```

**`ReplCallbacks` C 结构**（稳定 ABI）：

```c
typedef struct {
  // 返回候选：NUL 分隔的 UTF-8，cdylib 用完调 free_cb 释放；ctx 透传
  char* (*complete)(void* ctx, const char* line, size_t pos);
  // 返回动作串（"dedent"/"insert:…"/"replace:…"/"accept"/"newline:…"/""）
  char* (*key_edit)(void* ctx, const char* key, const char* line, size_t pos);
  void  (*free_str)(char* s);   // 释放上面两个回调返回的串（跨-allocator）
} ReplCallbacks;
```

**关键语义点（design 定清）**：

1. **线程/GC**：`readline` 阻塞期 z42vm 主线程本应 GC-park（native 调用惯例）。回调重入 VM 时须临时
   unpark（现 `repl_editing.rs` 已有 `NativeUnparkGuard::exit`）——此逻辑留 z42vm 的 trampoline，cdylib
   不碰 GC。
2. **异常穿透**：z42 回调里 `throw` → trampoline `set_pending_thrown` + 返回哨兵；cdylib 见哨兵中止
   readline、把 pending thrown 交回 z42vm（同现 `__z42_reflected_throw__` 路径）。
3. **ACTIVE_CTX**：现补全器用 thread-local `ACTIVE_CTX` 拿 live ctx。迁 cdylib 后改为**显式 ctx 透传**
   （`z42_repl_readline` 的 `ctx` 参数）——更干净，去掉 thread-local 魔法。
4. **`Cmd` 不过边界**：现 `parse_action`（动作串→rustyline `Cmd`）**整体进 cdylib**（`Cmd` 是 rustyline
   类型）。z42 侧回调只返回**动作串**（已是 string 协议），cdylib 内 `parse_action` 译成 `Cmd`。天然适配。

## §4 懒加载（首次调用才 dlopen——满足「不影响启动」）

现 `native/ext.rs::load_all` 在 VM 启动时**急切** dlopen 所有 `libz42_*`。compression 可接受；**REPL 不行**
（急切 dlopen repl 库反而给启动加成本）。故：

- **REPL 库懒加载**：`__repl_*` 保留为 z42vm **静态 builtin 薄壳**；其 body 首次调用时 `OnceLock` 懒
  dlopen `libz42_repl` + 缓存句柄 + 解析符号。启动路径完全不碰 repl 库。
- **repl 专用发现路径（不走 `native_search_paths()`）**：因 repl cdylib 打包在 **z42i 同侧的 toolchain
  子目录**（非 `<sdk>/native/`），懒 dlopen 按序找：① `Z42_REPL_NATIVE`（显式覆盖，CI/dev）② z42i 可执行
  文件同侧目录（`current_exe().parent()`——REPL 运行时 `current_exe` 是 z42i apphost，cdylib 就在旁边）
  ③ dev 布局 `artifacts/build/.../` 兜底。找不到 → repl builtin 回退「plain-stdin 无编辑」degrade（不 abort，
  与 wasm fallback 同语义）。
- compression（跨平台 runtime 扩展）维持急切 `native_search_paths()` + `<sdk>/native/` 不变——**两类库两套
  发现路径，互不影响**。
- 通用机制文档写明两类库（runtime 跨平台 vs toolchain host-only）各自的「打包位置 + 发现 + 急切/懒」策略。

## §5 通用度决策：Option A（推荐）vs Option B

- **Option A（沿 compression 范式 + 通用化文档 + 懒加载 + 回调）**：z42vm 对每个已知 native 库有一段
  wrap-shim 注册臂（`match name`）。新增库 = 加一个 crate + 一段注册臂 + 文档。**REPL 够用、风险低**、与既有
  compression 一致。**代价**：z42vm 仍「知道」每个库（非零 Rust shim）。
- **Option B（自描述插件 ABI）**：cdylib 导出「builtin 清单 + 每个的 marshal 签名 + 是否需回调」，z42vm 用
  通用 marshaler（类机制 3 的 libffi + descriptor）零 shim 加载**任意** native 库。**代价**：一大块通用
  marshaling ABI（尤其非 blittable 的 string / 回调），是月级投资；当前唯一新消费者只有 REPL。

**推荐 A**：YAGNI——只有 REPL 一个新消费者，不为它建大而全的插件系统；把「通用机制」作为**文档化的可复用
范式 + 第二个工作实例（REPL）**交付，已满足 User「设计通用机制」的诉求。若将来出现多个 native 库需求，再
按 B 投资（届时机制 3 的 libffi 派发可复用）。

## §6 REPL 具体拆分

| 现在（z42vm 内） | 迁移去向 |
|------------------|---------|
| rustyline `Editor` 装配 / `Config` / 键绑定（`repl.rs` `read_one_line`）| **cdylib** |
| `Completer` / `ReplHelper` / `KeyEditHandler`（rustyline trait 实现）| **cdylib** |
| `parse_action`（动作串→`Cmd`）| **cdylib**（`Cmd` 是 rustyline 类型）|
| history 加载/保存（rustyline `History`）| **cdylib** |
| 补全/键位**回 z42 重入**（`exec_function` / `ACTIVE_CTX` / GC park / `set_pending_thrown`）| **留 z42vm**（trampoline 回调）|
| `__repl_readline` / `__repl_set_completer` / `__repl_set_key_editor` builtin 薄壳 + 懒 dlopen | **留 z42vm** |
| `Std.Repl` z42 源（`Repl.z42` / `ReplEditing.z42`）| **不动**（`[Native("__repl_*")]` 名不变）|

- **z42 stdlib/toolchain 零改**：`__repl_*` 名与语义不变，`Std.Repl` 照旧。纯 runtime 内部重构。
- **wasm**：wasm 本就 cfg-gate repl 到 plain-stdin（无 rustyline）；cdylib 化后 wasm 直接不 bundle repl 库，
  fallback 不变。

## §7 值不值得（诚实评估，供裁决）

**实测**：z42vm release = 7.2 MB（含 rustyline + 19 crate）；一个 cdylib（compression）release dylib ≈ 2.1 MB。

| | 结论 |
|---|------|
| 启动时间 | 现已懒初始化、**不受影响**；cdylib + 懒 dlopen 保持不受影响。**无新改善** |
| z42vm 核心二进制 | 缩 rustyline 那份（估数百 KB–1 MB / 7.2 MB）|
| SDK 总磁盘 | **可能略增**（cdylib 重含一份 std/panic 运行时）|
| 核心 VM 依赖面 | rustyline + 19 crate **移出核心 crate 依赖图**（构建/审计/攻击面更小）——**主要真收益** |
| REPL fork | **不消除**（patch 在 rustyline 源码，随 cdylib 一起走）|
| 复杂度 | **新增**：cdylib crate + 双向 FFI re-entrancy（回调指针 + opaque ctx + GC/异常穿透）+ 懒 dlopen + 每平台打包 |

**我的建议（事实校正责任）**：真收益是**依赖隔离 + 确立通用 native 库范式**，不是体积/启动的戏剧性改善，
也不消除 fork。若你重视「核心 VM 依赖面干净 + 有一套可复用的 native 库范式」，值得做；若只图「启动更快 /
包更小」，收益不达预期——那种情况下更划算的是**直接保留 fork 落 `}`/floor**（启动本就不受影响）。请据此裁决。

## §8 Testing Strategy

- **cdylib 单元**：`crates/z42-repl` 内 rustyline 装配 / `parse_action` / Completer 逻辑的 Rust 单测
  （`cargo test -p z42-repl`）——含 `add-repl-rbrace-floor` 的 `parse_action`/`edit_insert_text` 光标测试
  （随 rebase 迁入）。
- **re-entrancy 回调**：z42vm 侧 trampoline 单测（mock cdylib 回调 → 断言 VmContext 重入 + 异常穿透 +
  GC unpark 正确）。
- **端到端**：`z42 repl -c "1+2"` dist smoke（现有）+ 交互验收（补全/键位/`}`/floor 手感）。
- **加载**：ext-loader 懒 dlopen 单测（首次调用才 open；缺库 fallback）。
- **GREEN**：`cargo test --lib` + `xtask test` + dist smoke；wasm bundled-repl 路径交 CI。
- **格式**：零 zbc/zpkg 格式 bump（纯 runtime 结构，不动格式）。
