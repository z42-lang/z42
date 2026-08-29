# Native 库的布局与解析（放哪、怎么找、发布期拍平）

本页讲**一个 native 动态库住在哪个目录、运行期怎么被定位到、多平台怎么随发布落地**——即 native 库
的*布局与解析*轴。它与[Native 扩展库（cdylib 机制）](native-extensions.md)正交：那页讲一个 native
库**内部怎么工作**（C ABI、回调、trampoline、如何新增一个），本页讲它**外部住哪、怎么被找到**。

z42 有**两类** native 库，走**两条**互不干扰的布局/解析路径：

| | 标准库 native（如 compression） | 组件私有 native（如 repl 的 libz42_repl） |
|---|---|---|
| 归属 | 运行时横切库，随 **SDK** 分发，不属某个 app zpkg | 跟随**某个组件 / app** 的私有依赖 |
| 布局 | `<sdk>/native/`（集中） | **平铺在消费方 zpkg 旁** `<zpkg-dir>/lib<name>.<suffix>` |
| 发现 | `ext::native_search_paths()` **急切扫描**目录 | `ext::resolve_native_beside(zpkg_dir, name)` **按名定向 stat** |
| 盲扫? | 是——扫目录里所有 `libz42_*` 并 `dlopen` | **否**——只 stat 被声明需要的那一个 |
| 本页焦点 | 现状不变（下 §1 概述） | 本 change 新增（§2/§3） |

> 设计出处：[add-path-dependencies Decision 9/10](../../../spec/changes/add-path-dependencies/design.md)
> ——native 库是「path 依赖」的另一半：path 依赖 colocate 私有组件的 **zpkg** 进 payload，native
> 库同族，也 colocate 在消费方 zpkg 旁。

---

## 1. 标准库 native：`<sdk>/native/` 急切扫描（现状不变）

compression 等是**运行时横切库**（跨平台、随 SDK 走、不属任何单个 app），布局与解析**本 change 不
碰**：

- **布局**：`<sdk>/native/libz42_<name>.{so,dylib,dll}`。
- **发现**：VM 启动 `ext::load_all` 扫 `native_search_paths()`（按序：`Z42_NATIVE_PATH` 覆盖 →
  `<exe>/../native/` SDK 布局 → `<exe>/native/` → `<exe>/` dev 直放），对每个 `libz42_*` 反解出
  `<name>`（`parse_z42_lib_name`）并 `dlopen` 注册已知符号集。**这是目录级盲扫**——扫到什么就
  `dlopen` 什么，未知库只会喷一条 `ignoring unknown lib <name>` WARN。
- **确定序**：扫描前必须 `sort`（common-pitfalls §1：`read_dir` 顺序依赖 OS/FS，first-wins 注册
  会非确定）。

急切盲扫对「随 SDK 分发的一小撮已知横切库」够用，但它是 repl WARN 污染的根源（§2 讲为什么把
repl 移出这条路径）。

---

## 2. 组件私有 native：平铺在消费方 zpkg 旁 + 按名定向解析

一个组件（interactive/app）可能带**私有** native 库——只有它自己用，不该进 `<sdk>/native/` 被全局
盲扫。布局与解析：

- **运行期布局唯一 = 平铺**：`<消费方 zpkg 目录>/lib<name>.<平台后缀>`，与该组件的 zpkg 同目录。
  无 rid 子目录、无嵌套——运行期永远只看「zpkg 旁那一个文件」。
- **发现 = 按名定向 stat**：`ext::resolve_native_beside(zpkg_dir, lib_name)` 用 `DLL_PREFIX`/
  `DLL_SUFFIX` 反向拼出 `lib<name>.<suffix>`，**只 stat 这一条路径**（存在→返回全路径，否则 `None`）。
  **不遍历目录、不 `dlopen` 任何未被点名的文件**——从根消除了 §1 那种「盲扫到未知库 → WARN /
  跨污染」。
- **多 rid → 发布期拍平**（非运行期选择）：运行期不做 rid 子目录挑选；由 `z42b publish` 按**目标
  rid**把对的那个 native 平铺进消费方 dist（镜像 zpkg 的 `_pubBundleProjectDeps`）。移动端复制到
  OS 约定目录（Android `jniLibs/<abi>`、iOS framework），运行期交 OS loader。→ 运行期 resolver
  只有「平铺 beside zpkg」这一条路径，dead simple。

```rust
// src/runtime/src/native/ext.rs
pub(crate) fn resolve_native_beside(zpkg_dir: &Path, lib_name: &str) -> Option<PathBuf> {
    let file = format!("{}{}{}", DLL_PREFIX, lib_name, DLL_SUFFIX); // 反向拼 lib<name>.<suffix>
    let candidate = zpkg_dir.join(file);
    candidate.is_file().then_some(candidate)                        // 单一 stat，不扫目录
}
```

### 2.1 唯一实例：REPL 行编辑器 cdylib（`libz42_repl`）

当前唯一的组件私有 native 库是 REPL 的 `libz42_repl`（host-only 行编辑器 cdylib，详见
[native-extensions.md §2](native-extensions.md#2-双向范式z42-repl)的 C ABI / 回调机制）。它的
「消费方 zpkg 目录」= interactive apphost 的 payload 目录 **`<sdk>/programs/z42i/`**（`z42i` 的
`z42.interactive.zpkg` 就在那）。

- **打包**（`scripts/package/`）：`_pkgStageReplCdylib` 把 `libz42_repl` 拷进 z42i 组件的
  `programs/z42i/`（**不是** `bin/`）；`_pkgStageZ42vm` **不**再往 `bin/` 放 repl 库；`_copyNativeLibs`
  的 `libz42*` glob 显式排除 repl（它不进 `<sdk>/native/`）。
- **发现**（`corelib::repl_native::candidates()`）：`Z42_REPL_NATIVE` 覆盖 → dev cargo-target
  目录（z42vm 旁）→ 从运行 `<sdk>/bin/<app>` 派生 `<sdk>/programs/z42i/`，经共享的
  `resolve_native_beside` 解析。
- **为什么移出 `bin/`**：repl 库曾与通用 z42vm 同处 `bin/`，被 §1 的急切 ext 扫描器 `dlopen` →
  `ignoring unknown lib repl` WARN，SDK VM 跑任何程序都喷、污染 golden。物理隔离到
  `programs/z42i/`（通用 z42vm 的 `exec_dir` 扫描不含它）根治了这条 WARN，同时把 repl 归入
  「组件私有 native」这条正规路径。

> repl **不是** `[Native(lib=)]` ext-builtin（那是 §1 的注册式扩展），而是带回调 C ABI 的专用
> host-editor cdylib，由 repl 子系统自己 `dlopen`。与本页共享的只是**路径解析层**
> （`resolve_native_beside`）；repl 的回调式加载仍是专用的。

---

## 3. app 声明面：`[native.dependencies]`（Deferred）

上面的 colocation/解析地基已就位，但「app 在 manifest 里**声明**它需要某个私有 native 库」的**声明面
尚未落地**（`[native.dependencies]`）。原因：当前唯一的非标准库 native 消费者是 repl，由 packaging
直接放置（`_pkgStageReplCdylib`），没有走 app 声明。

- `builder_publish.z42` 已留 `_pubBundleProjectNativeDeps` **骨架**（挂在 `_pubBundleProjectDeps`
  邻位，当前 no-op），待声明面落地后按「声明的 native 依赖 + 目标 rid → 平铺进消费方 dist」充实。
- 详见 [add-path-dependencies Deferred](../../../spec/changes/add-path-dependencies/design.md)
  的 `native-future-app-declaration`。

---

> 相关：[Native 扩展库（cdylib 机制）](native-extensions.md)、[加载上下文（LoadContext）](load-context.md)、
> [REPL 输入完整性判定](../toolchain/repl-input-completeness.md)。
