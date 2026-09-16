# Proposal: add z42 WebAssembly playground

## Why

z42 currently has no zero-install way to try the language. Every new user has
to `git clone` + build dotnet + cargo + stdlib → high friction for evaluation,
docs samples,教程 / blog post 链接、issue 重现等。

Web playground 解决：
- 用户点 URL → 加载页面 → 直接编辑代码 + 点 Run 看输出
- README / docs 里可以嵌可运行 snippet
- Issue 报告附 playground 链接而非源码片段

basement infrastructure 已就位（Cargo `wasm` feature + `wasm32-unknown-unknown`
target + roadmap 已列 `browser-wasm` 分发），只缺 wasm-bindgen 层 + 编译 API +
浏览器 UI。

## What Changes

新增三个 sibling 组件，组合成一个可独立部署的 playground：

1. **`src/toolchain/playground/wasm/`** — Rust crate，wasm-bindgen wrapper 包
   `z42vm`，导出 `run_zbc(bytes, entry, libs) → ExecResult` 给 JS 调
2. **`src/toolchain/playground/server/`** — .NET ASP.NET minimal API HTTP service，
   唯一 endpoint `POST /compile`：源码进 → zbc bytes 出。**in-process** 调
   `Z42.Compiler.PlaygroundCompiler.CompileSource(...)`（无 subprocess fork）—
   这条 in-memory compile API 同时是未来 Blazor fallback 路径的前置基础
3. **`src/toolchain/playground/web/`** — Vite + vanilla TS + Monaco editor，
   编辑器 + Run 按钮 + stdout/stderr panel；首次加载 fetch stdlib zpkgs +
   wasm module，后续 source 改动只走 `/compile` API

非编译器 / VM / stdlib 语义变更 —— 100% 新增 surface，复用现有 z42c binary +
z42vm Rust code。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/playground/wasm/Cargo.toml`              | NEW    | z42-wasm crate manifest（wasm-bindgen dep） |
| `src/toolchain/playground/wasm/src/lib.rs`              | NEW    | wasm-bindgen exports：run_zbc + ConsoleSink |
| `src/toolchain/playground/wasm/README.md`               | NEW    | 怎么 wasm-pack build |
| `src/toolchain/playground/server/Z42.Playground.Server.csproj` | NEW    | ASP.NET minimal API project；ProjectReference z42.Driver / Pipeline / Project / IR / Semantics / Syntax / Core |
| `src/toolchain/playground/server/Program.cs`            | NEW    | minimal API：POST /compile in-process + CORS + rate limit + 5s CancellationToken |
| `src/toolchain/playground/server/README.md`             | NEW    | 怎么 `dotnet run` + Docker deploy |
| `src/compiler/z42.Compiler/PlaygroundCompiler.cs`       | NEW    | in-memory compile API：`CompileSource(string source, IReadOnlyDictionary<string,byte[]> stdlibZpkgs) → CompileResult`。**核心改动**，给 server 和未来 Blazor 共用 |
| `src/compiler/z42.Pipeline/SingleFileCompiler.cs`       | MODIFY | 抽出 fs-walk 层，让 PlaygroundCompiler 复用 fs-free 部分 |
| `src/toolchain/playground/web/package.json`             | NEW    | Vite + monaco-editor + ts deps |
| `src/toolchain/playground/web/index.html`               | NEW    | 单页 |
| `src/toolchain/playground/web/src/main.ts`              | NEW    | 主逻辑 + wasm load + API call |
| `src/toolchain/playground/web/src/z42-monaco.ts`        | NEW    | Monaco language def（关键字 / 字符串 / 注释 high） |
| `src/toolchain/playground/web/README.md`                | NEW    | 怎么 npm run dev / build |
| `src/toolchain/playground/README.md`                    | NEW    | playground/ 总览（架构 + 怎么本地跑端到端） |
| `src/runtime/Cargo.toml`                                | MODIFY | 若需要 expose 额外 pub API for wasm wrapper（例如 stdlib zpkg loader） |
| `docs/internals/src/runtime/embedding.md`                      | MODIFY | 加 playground 一节说明 wasm 分发产物如何生成 |
| `docs/roadmap.md`                                       | MODIFY | playground 标为已落地 |
| `scripts/build-playground.sh`                           | NEW    | 端到端 build：wasm-pack + cargo + npm build |

**只读引用**（不修改，但要理解）：

- `src/runtime/src/lib.rs` — 看现有公开 API（vm::Vm / vm_context::VmContext）
- `src/runtime/src/main.rs` — 看 z42vm CLI 怎么 load zpkg + run，wasm wrapper
  复用同样的 load 路径
- `src/compiler/z42.Driver/` — z42c CLI 入口（保持 binary 形态）；server **不再
  fork subprocess**，直接通过 ProjectReference + `PlaygroundCompiler.CompileSource`
  调用
- `versions.toml` `[platform.wasm]` — wasm_pack 版本 pin 必须 match
- `docs/internals/src/runtime/embedding.md` 第 11.9 节 — 分发 package 形态约定

## Out of Scope

- **z42c → WASM**（Blazor）：架构 A，未来 issue 触发再做。本期走架构 B：z42c
  留在 server，浏览器只跑 vm
- **Monaco z42 完整 LSP**：本期只 token-level 高亮（keyword / string / comment）；
  goto-def / hover / autocomplete 留 follow-up（需要 z42c 在浏览器跑或开 LSP
  API）
- **多文件 project / workspace 编辑**：本期单文件 source 编辑；多文件支持留
  follow-up
- **持久化 / share-via-URL**：本期 ephemeral；分享功能（serialize source →
  short URL）留 follow-up
- **真实运行时输入（stdin）**：本期 Console.WriteLine 单向输出；Console.ReadLine
  返回空字符串
- **z42.io 文件系统 / Process / Directory**：浏览器无 fs / fork，浏览器侧
  builtin 直接 `bail!("not supported in playground")`，不试图 emulate

## Open Questions

- [ ] **stdlib 大小**：当前 16 个 zpkg ~340KB（gzip ~120KB？需测）。可接受作 first-page
  bundle，还是 lazy-load 用到的子集？v0 全 bundle 简单 → 用 lazy 留 follow-up
- [ ] **server 部署位置**：repo CI 还是手工托管？v0 文档教 docker run，正式
  上线由 ops 决定
- [ ] **rate limit 策略**：每 IP /s 多少 compile？v0 简单 token-bucket 1 req/s
  per IP，被滥用再升级
- [ ] **stdout buffer 限制**：无限循环 print 不能拖死浏览器；硬上限 1MB 或
  10s timeout，触发后停 vm
