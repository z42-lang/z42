# Tasks: WASM 内存 VFS 后端

> 状态：🔴 DRAFT（spike 已验证）；待 User 6.5 裁决 D1–D3 | 创建：2026-07-29
> 拟占子系统：`runtime` + `toolchain`

## 已完成（spike，`wasm-vfs-spike` 分支）
- [x] `corelib/vfs.rs` 内存 VFS + `__vfs_mount`/`__vfs_enable`
- [x] `fs.rs` 3 个 DepScan builtin（read_bytes/dir_exists/path_glob）路由 VFS
- [x] **实测证明**：native 挂 36 zpkg → DepScan.ScanDirs("/vfs") → ns=43 modules=374（与磁盘一致）VFS_DEPSCAN_OK=1

## 决策待定（见 proposal.md）
- D1 fs 后端：cfg(wasm32) 默认 VFS(A,推荐) vs FsBackend trait(B)
- D2 z42c 打包：lazy fetch(A,推荐) vs 全 bundle(B)
- D3 compile 入口：直接暴露 Script.Eval(推荐) vs 新做 compile(source)

## 阶段（D1=A/D2=A/D3=前者，待确认后回填）
- [ ] 阶段 1 — fs.rs 全量路由：32 个 fs builtin 按 cfg(wasm32) 走 VFS；非只读 op 优雅降级（明确 not-supported）
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
