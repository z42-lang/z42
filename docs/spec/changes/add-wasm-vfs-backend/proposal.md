# Proposal: WASM 内存 VFS 后端——解锁浏览器内编译（playground 基础支持）

> 状态：🔴 DRAFT（spike 已验证核心假设；待 User 6.5 裁决 D1–D3）
> 创建：2026-07-29 | 拟占子系统：`runtime`（fs 后端 + wasm-bindgen）+ `toolchain`（wasm facade / 打包）

## Why

WASM playground（网站在**别的仓库**，本仓库只做**基础支持**：能在浏览器 wasm 里编译+运行 z42 的引擎 + JS↔WASM 接口）。调研发现：

- **已就绪的很多**：wasm 构建管线、`Z42VM.loadZbc/invoke`（跑预编译 zbc）、`ZpkgResolver`（内存喂 stdlib）、`io.rs` HostSink（Console→JS）、GC 纯 Rust 自管堆（wasm 下原样跑，**非** blocker）。
- **唯一大 blocker**：`Script.Eval` 那条"在 VM 里编译每行"的回路，其编译期依赖世界 `DepScan` **绑死文件系统**（`File.ReadAllBytes`/`Directory.Exists`/`Path.Glob` + `Z42_LIBS` 磁盘目录），浏览器无 fs。
- **既有 `add-z42-wasm-playground` 提案已过时**：它选架构 B（编译留 **.NET server** 调 `Z42.Compiler.PlaygroundCompiler`），但 **C# 编译器已于 2026-06-26 整个删除**（z42 自举）——那条路的 server 组件已不存在。⚠️ 规范冲突，本提案取代之。

**核心洞察（User 提出，spike 已证）**：不必重写 DepScan 变 fs-free，也不必搭 server——`fs.rs` 是**单一 fs 收口点**，把它路由到**内存 VFS**（path→bytes），则 DepScan / z42c / scripting 引擎**一行不改**即可在无 fs 的 wasm 里编译。这正是 Pyodide / emscripten MEMFS / WASI 的模式。

## Spike 实据（已提交 `wasm-vfs-spike` 分支）

- `corelib/vfs.rs`：进程级内存 VFS（`path→bytes`）+ `__vfs_mount`/`__vfs_enable` builtin。
- `corelib/fs.rs`：DepScan 用到的 3 个 builtin（`file_read_bytes`/`dir_exists`/`path_glob`）加 `vfs::enabled()` 分支。
- **实测（native）**：挂 36 个 zpkg 进 VFS → `enable` → `DepScan.ScanDirs("/vfs")` → **ns=43 modules=374（与磁盘全量一致）VFS_DEPSCAN_OK=1**。

→ **证明：DepScan 在纯内存 zpkg 上跑通。** 最大 blocker 解除。

## 提议的架构：VFS-enabled 浏览器内编译（架构 A，现可行）

不走 server；playground = 纯前端 + wasm，零后端。本仓库提供的**基础支持**：

1. **fs 后端抽象**（D1）：`fs.rs` 的 fs builtin 在 wasm 下默认走内存 VFS。两种落法：
   - (A) `cfg(target_arch="wasm32")` 直接默认 VFS（native 不变，零开销）；
   - (B) `FsBackend` trait（`NativeFs`/`MemoryVfs`/`JsCallbackFs`），运行时选后端——对齐既有 `ZpkgResolver` 模式，更通用（JsCallbackFs 可按需从网站 fetch zpkg）。
2. **JS 挂载接口**（wasm-bindgen）：`Z42VM` 加 `mountZpkg(path, bytes)` / 或构造时接 `zpkgResolver` 同源——网站把 stdlib + **z42c 编译器** zpkg 灌进 VFS。
3. **z42c 编译器 zpkg 进 wasm 分发**：`package-wasm` 现只打 stdlib，需**加打 z42c.*（+ z42.scripting）**——浏览器内编译需要编译器包在 VM 里。
4. **compile+run 入口**：`Z42VM.eval(source)` 或直接暴露 `Std.Scripting.Script.Eval`——`Z42_LIBS` 设成虚拟目录（如 `/vfs`），scripting 引擎原样跑。
5. **非编译 fs op 的优雅降级**：VFS 只读；写/删/网络等在 wasm 下明确 `not supported`（现 fs.rs 未 gate → 运行期崩，应改明确报错）。
6. **复用**：VFS 的字节源与运行期 `ZpkgResolver` **共用同一份挂载 zpkg**（编译期 fs 读 + 运行期 zpkg 解析，一份数据两处用）。

## 待 User 裁决（6.5 gate）

- **D1｜fs 后端形态 → 定为平台隔离后端（User 2026-07-29：按平台拆分、隔离不同实现）**：
  采纳 **Rust std `sys/` 模式**——`corelib/fs/` 拆成平台无关 builtin 层（`mod.rs`）+ 隔离的平台实现
  （`native.rs` std::fs / `memory.rs` path→bytes VFS / 未来 wasi·mobile）+ 后端选择（`backend.rs`）。
  - builtin 层平台无关：只调 `backend().read/write/exists/glob`，无 inline `std::fs` / `if wasm`。
  - **cfg 选默认 + 运行时可覆盖**：`cfg(wasm32)`→MemoryVfs，否则 NativeFs；保留 `set_backend()` 供
    一致性测试在 native 强制 MemoryVfs（内存 vs 磁盘一致，快 CI），wasm 内亦可切 Memory/JsCallback。
  - enum（`Native/Memory/JsCallback` + 方法内 match，无 vtable）或 trait 皆可；fs I/O 密集低频，开销忽略。
  - **spike 的 inline `vfs::enabled()` 分支是临时糙形**（native 快速验证用）→ IMPL 第一步重构成本隔离结构。
  - ~~(A) inline cfg~~ 弃：32 个 builtin 全塞平台分支太乱。
- **D2｜z42c 打包 vs lazy fetch**：编译器 zpkg 几 MB。
  - **(A, 推荐)** lazy fetch：首次编译时才从网站拉 z42c zpkg（配 JsCallbackFs 或前端预取）。
  - (B) 全 bundle 进 wasm 包（简单但首屏下载大）。
  - 注：本仓库只需**产出** z42c zpkg 静态产物 + 暴露挂载接口；拉取策略网站定。
- **D3｜compile 入口**：直接暴露 `Script.Eval`（REPL 引擎，含惰性 scan #65）vs 新做一个更薄的 `compile(source)→zbc`。推荐前者（复用刚优化的 scripting 引擎，一次到位 REPL + playground）。

## Scope（D1=A/D2=A/D3=前者 下）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/corelib/vfs.rs` | NEW（spike 已有）| 内存 VFS + mount/enable；生产化：wasm 下默认启用 |
| `src/runtime/src/corelib/fs.rs` | MODIFY（spike 已改 3 个）| 全部 32 个 fs builtin 按 `cfg(wasm32)`/`vfs::enabled()` 路由；非只读 op 优雅降级 |
| `src/toolchain/workload/wasm/platform/src/lib.rs` | MODIFY | `Z42VM` 加 `mountZpkg` / `eval(source)`；接 VFS |
| `scripts/package/xtask_package_wasm.z42`（或等价）| MODIFY | wasm 分发加打 z42c.* + z42.scripting zpkg |
| `docs/design/` + `docs/workflow/building/wasm.md` | MODIFY | VFS 后端机制页；playground 基础支持接口文档 |
| `docs/spec/changes/add-z42-wasm-playground` | 取代/重定位 | 标注过时（C# server 已删）→ 指向本 change |
| 测试 | NEW | VFS DepScan 一致性（内存 vs 磁盘一致）；wasm facade eval e2e（Playwright） |

## 子系统 / 锁

`runtime`（主）+ `toolchain`（wasm facade / 打包）。**runtime 现状**：ACTIVE.md 查空闲则登记；spike 已在 `wasm-vfs-spike` 隔离分支。

## 非目标 / 后续
- 网站 UI / Monaco / 分发缓存（**别的仓库**）。
- `FsBackend` trait 通用化（D1=B）——可后续。
- JIT（wasm W^X 禁 → interp-only，编译器慢；惰性 scan #65 已大幅缓解，首次编译估计几秒，playground 可接受）。
- mobile scripting：**同一 VFS 也解锁它**（iOS/无 fs），一份投入两处用。

## 风险
- **interp 编译性能**：wasm interp 比 native 慢；首次 scan 已由 #65 从 4.3s 降到 native 1.4s，wasm 下更高但可接受（playground 非高频）。若烦，配前端"编译中"提示 + Web Worker（网站侧）。
- **下载体积**：z42c zpkg 几 MB → lazy fetch（D2=A）。
- **VFS 只读足够编译**：DepScan 只读；scripting 用内存加载不落盘（`__load_bytecode_in_memory`），故 VFS 无需写。写类 fs op 在 wasm 明确不支持。
