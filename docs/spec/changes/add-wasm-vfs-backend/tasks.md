# Tasks: WASM 内存 VFS 后端

> 状态：🔴 DRAFT（spike 已验证）；待 User 6.5 裁决 D1–D3 | 创建：2026-07-29
> 拟占子系统：`runtime` + `toolchain`

## 已完成（spike，`wasm-vfs-spike` 分支）
- [x] `corelib/vfs.rs` 内存 VFS + `__vfs_mount`/`__vfs_enable`
- [x] `fs.rs` 3 个 DepScan builtin（read_bytes/dir_exists/path_glob）路由 VFS
- [x] **实测证明**：native 挂 36 zpkg → DepScan.ScanDirs("/vfs") → ns=43 modules=374（与磁盘一致）VFS_DEPSCAN_OK=1

## 决策
- **D1 已定（User 2026-07-29）**：平台隔离后端（Rust std `sys/` 模式）——`corelib/fs/` 拆平台无关
  builtin 层 + 隔离的 native/memory 实现 + cfg-默认·运行时可覆盖。**不用** inline cfg。
- D2 z42c 打包：lazy fetch(A,推荐) vs 全 bundle(B)｜D3 compile 入口：暴露 Script.Eval(推荐) vs 新 compile

## 阶段（D2=A/D3=前者，待确认后回填）
- [ ] 阶段 1 — **fs 平台隔离重构**：`corelib/fs.rs` → `corelib/fs/{mod,backend,native,memory,glob}.rs`；
  32 个 builtin 改调 `backend()`（无 inline std::fs / wasm 分支）；cfg 默认 + `set_backend` 覆盖；
  memory.rs 由 spike 的 vfs.rs 演进；非只读 op 在 memory 后端优雅降级（明确 not-supported）。
- [ ] 阶段 2 — wasm facade：`Z42VM.mountZpkg(path,bytes)` + `eval(source)→输出`；Z42_LIBS=/vfs
- [ ] 阶段 3 — 打包：wasm 分发加 z42c.* + z42.scripting zpkg 静态产物（package-wasm）
- [ ] 阶段 4 — 测试：VFS DepScan 一致性（内存 vs 磁盘）+ wasm eval e2e（Playwright，编译一段 z42 源→输出）
- [ ] 阶段 5 — 文档：VFS 后端机制页 + wasm.md 基础支持接口 + 取代过时的 add-z42-wasm-playground
- [ ] 阶段 6 — 取代 `add-z42-wasm-playground`：标注过时（C# server 已删），指向本 change

## 前置 / 依赖
- 依赖 #65（惰性 scan）缓解 interp 编译慢——已合并 main。
- 网站 UI / 分发缓存在别的仓库；本 change 只出**基础支持**（VFS + 挂载接口 + z42c zpkg 产物 + eval 入口）。

## 后续（不做）
- FsBackend trait 通用化（D1=B）｜mobile scripting（同 VFS 复用）｜JIT（wasm 无）。
