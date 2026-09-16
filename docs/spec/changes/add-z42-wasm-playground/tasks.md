# Tasks: add z42 WASM playground

> 状态：🟡 进行中 | 创建：2026-05-20 | 类型：feat（新组件，无现有语义变更）
> Spec 类型：完整（proposal + design + tasks，但无 spec.md —— 无新语言行为）

## 阶段 1: PlaygroundCompiler in-memory API + z42-wasm crate

### 1A: z42c in-memory entry（**新增**，给 server + 未来 Blazor 共用）

- [ ] 1A.1 NEW `src/compiler/z42.Compiler/PlaygroundCompiler.cs`
  - public `CompileSource(string source, IReadOnlyDictionary<string,byte[]> stdlibZpkgs, CancellationToken) → CompileResult`
  - public record `CompileResult(byte[]? ZbcBytes, IReadOnlyList<Diagnostic> Diagnostics, long CompileTimeMs)`
- [ ] 1A.2 MODIFY `src/compiler/z42.Pipeline/SingleFileCompiler.cs`
  - 抽出 fs-free 核心：`CompileCu(CompilationUnit cu, DepIndex depIndex, ImportedSymbols imported) → IrModule`
  - 既有 `Run(FileInfo source, ...)` 改为薄 wrapper（读文件 → 喂 fs-free 核心）
  - 新增 `LocateDepIndexFromBytes(IReadOnlyDictionary<string,byte[]> zpkgs)` / `LocateImportedSymbolsFromBytes(...)` —— 不 walk fs，直接接 byte[]
- [ ] 1A.3 MODIFY `src/compiler/z42.Driver/Program.cs` 仅必要：z42c CLI 继续走 SingleFileCompiler.Run（行为不变）
- [ ] 1A.4 NEW `src/compiler/z42.Tests/PlaygroundCompilerTests.cs`
  - test hello world：source + 16 stdlib zpkg bytes → CompileResult.ZbcBytes != null
  - test compile error：bad source → Diagnostics 含 line/col/message
  - test cancellation：infinite-loop fixture + 100ms token → 抛 OperationCanceledException
- [ ] 1A.5 VERIFY: `dotnet test src/compiler/z42.Tests` 全过；现有 1288 tests 不回归

### 1B: z42-wasm crate（VM in browser）

- [ ] 1B.1 NEW `src/toolchain/playground/wasm/Cargo.toml`
  - workspace member; deps = `z42 = { path = "../../../runtime", features = ["wasm"] }` + `wasm-bindgen` + `console_error_panic_hook` + `serde-wasm-bindgen`
- [ ] 1B.2 NEW `src/toolchain/playground/wasm/src/lib.rs`
  - `PlaygroundVm` struct + constructor 接收 stdlib zpkgs (parallel `names: string[]` + `bytes: Uint8Array[]`)
  - `run_zbc(zbc_bytes, entry, timeout_ms) -> ExecResult` 入口
  - ConsoleSink redirect stdout/stderr 到内部 buffer
  - Timeout 实现：用 `js_sys::Date::now()` 在 interp loop 里周期检查（无 thread）
- [ ] 1B.3 NEW `src/toolchain/playground/wasm/README.md` — `wasm-pack build --target web --release` 入门
- [ ] 1B.4 VERIFY: `wasm-pack build` 成功 + 产物 `pkg/z42_wasm_bg.wasm` 存在 + 大小 < 5MB（pre wasm-opt）

## 阶段 2: server project（ASP.NET in-process API）

- [ ] 2.1 NEW `src/toolchain/playground/server/Z42.Playground.Server.csproj`
  - `<Sdk>Microsoft.NET.Sdk.Web</Sdk>` (ASP.NET Core minimal API)
  - ProjectReference: `..\..\..\compiler\z42.Compiler\z42.Compiler.csproj`（间接拉 Pipeline / IR / Semantics / Syntax / Core / Project）
  - PackageReference: `Microsoft.AspNetCore.RateLimiting`（.NET 7+ 内置，可能不需显式）
- [ ] 2.2 NEW `src/toolchain/playground/server/Program.cs`
  - 启动时 load 16 stdlib zpkgs 到 static `IReadOnlyDictionary<string, byte[]>`
  - `POST /compile`：JSON `{source: string}` → in-process `PlaygroundCompiler.CompileSource(...)` → 200 + `{zbc: base64, durationMs}` 或 4xx + `{diagnostics: [...]}`
  - `GET /libs/{name}.zpkg`：static serve from same dict（dev only；prod 走 CDN）
  - `CancellationTokenSource` 5s per-request timeout
  - `UseRateLimiter()` 1 req/s per IP burst 5
  - `UseCors()` dev `*`、prod 配置 origin
  - `UseExceptionHandler()` catch panic 转 5xx（进程不死）
  - `RequestSizeLimit(256KB)` 防 source bomb
- [ ] 2.3 NEW `src/toolchain/playground/server/README.md` — `dotnet run` + Docker deploy
- [ ] 2.4 NEW `src/toolchain/playground/server/Dockerfile` — multi-stage `dotnet publish` + minimal runtime image
- [ ] 2.5 VERIFY: `dotnet run --project src/toolchain/playground/server` + curl 测 hello world compile

## 阶段 3: web frontend (UI)

- [ ] 3.1 NEW `src/toolchain/playground/web/package.json` — Vite + monaco-editor + TypeScript + serve
- [ ] 3.2 NEW `src/toolchain/playground/web/index.html` — 单页 layout：左编辑器、右上 Run + Reset、右下 Output
- [ ] 3.3 NEW `src/toolchain/playground/web/src/main.ts`
  - import wasm pkg via `import init from '../wasm/pkg/z42_wasm.js'`
  - 并发 fetch 16 zpkgs → IndexedDB cache
  - new PlaygroundVm(zpkgs)
  - editor onRun handler
- [ ] 3.4 NEW `src/toolchain/playground/web/README.md`
- [ ] 3.5 VERIFY: `npm run dev` → 浏览器 localhost:5173 → type hello world → Run → 看到 stdout

## 阶段 4: Monaco z42 language definition

- [ ] 4.1 NEW `src/toolchain/playground/web/src/z42-monaco.ts`
  - port z42 keyword list from `src/compiler/z42.Syntax/Lexer/TokenDefs.cs::Keywords`
  - monarch tokenizer rules：keywords / string literals / 单行 + 多行 comments / 数字 / identifiers
- [ ] 4.2 VERIFY: 在浏览器手测高亮（关键字着色）

## 阶段 5: 端到端 build + docs

- [ ] 5.1 NEW `scripts/build-playground.sh`
  - wasm-pack build src/toolchain/playground/wasm
  - dotnet publish src/toolchain/playground/server -c Release -o artifacts/playground/server
  - cd src/toolchain/playground/web && npm install && npm run build
  - 输出汇总到 `artifacts/playground/{wasm, server, web}`
- [ ] 5.2 MODIFY `docs/internals/src/runtime/embedding.md` — 加 `### Playground` 一节说明 wasm 分发产物
- [ ] 5.3 MODIFY `docs/roadmap.md` — playground 标已落地（"workflow / 工具链" 段）
- [ ] 5.4 NEW `src/toolchain/playground/README.md` — 总览：架构图 + 三 component 各自怎么 build + 怎么本地起 + 怎么 deploy

## 阶段 6: GREEN + 归档

- [ ] 6.1 `wasm-pack build src/toolchain/playground/wasm` 成功
- [ ] 6.2 `dotnet publish src/toolchain/playground/server -c Release` 成功
- [ ] 6.3 `cd src/toolchain/playground/web && npm run build` 成功
- [ ] 6.4 本地端到端：起 server + start web + 浏览器 hello world 看到 "Hello, world"
- [ ] 6.5 现有 `./scripts/test-stdlib.sh` + `cargo test` + `dotnet test` 不回归
- [ ] 6.6 mv → `docs/spec/archive/2026-05-20-add-z42-wasm-playground/`
- [ ] 6.7 commit + push

## 阶段 7（follow-up，不在本 spec）

- Blazor 路径作 offline fallback
- Monaco LSP（hover / goto-def）
- 流式 stdout
- share-via-URL
- 多文件 project
