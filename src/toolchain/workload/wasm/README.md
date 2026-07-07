# @z42/wasm — WebAssembly facade for the z42 embedding API

> 状态：🟢 H4 落地（2026-05-12）。
>
> Spec：[`docs/spec/archive/2026-05-12-add-platform-wasm/`](../../../../docs/spec/archive/2026-05-12-add-platform-wasm/)
> 跨平台契约：[`../README.md`](../README.md)
> 实现原理：[`docs/design/runtime/embedding.md`](../../../../docs/design/runtime/embedding.md) §6.2 / §11

把 z42 VM 包成 WebAssembly + JS facade，供浏览器 / Node.js / wasm-runtime 一行 `import` 即可跑 `.zbc` 字节码。

## Quick Start

### 一次性环境准备

```bash
# Rust target + wasm-pack（如果还没装）
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked

# 编译器 + stdlib（产出 .zpkg 字节）
dotnet build src/compiler/z42.slnx
```

### 构建 + 跑 demo

```bash
# 一站式构建（pkg-web/pkg-nodejs via wasm-pack）+ 测试资产（fixture .zbc + stdlib）：
./xtask test platform wasm build      # ① wasm-pack web/node 两 target
./xtask test platform wasm assets     # ② 编 fixtures + 拷 stdlib + files.json

cd src/toolchain/workload/wasm/platform
# 跑浏览器 demo（无需 Node；任选一个静态服务器）：
miniserve --index demo/web/index.html .        # 然后开 http://127.0.0.1:8080/
# 或：dotnet serve -p 8000                      # 开 http://127.0.0.1:8000/demo/web/index.html
# 或：python3 -m http.server 8000               # 同上 URL

# 或跑 Node demo（需要本地 Node — 走 artifacts/tools/node）
./xtask deps install --os wasm                # 一次性，node 属 wasm 必备，装到 artifacts/tools/node
PATH="$PWD/../../../../artifacts/tools/node/bin:$PATH" node demo/node/run.js
# 期望输出：[host] hello, world
```

> 详细 step-by-step 跑通流程见 [`docs/workflow/building/wasm.md`](../../../../docs/workflow/building/wasm.md)。

`[host]` 前缀来自 demo 注册的 stdout handler，证明输出**经过宿主回调**而不是 wasm 内部 println。

## Run tests

```bash
# 每次（自动 build + assets + pkg-nodejs Node smoke + Playwright R1–R7；
# 本地 Node 缺失时自动装到 artifacts/tools/，不动系统）：
./xtask test platform wasm
```

期望尾部输出：

```
Running 7 tests using 1 worker
  ✓  1 [chromium] › r1-r7.spec.ts:14:1 › R1 smoke / hello world
  ✓  2 ... R2 error / bad zbc throws status 10
  ...
  7 passed (4.0s)
```

playwright 在 headless chromium 中跑 7 个 platform-test-contract scenario（详见 [`add-wasm-tests`](../../../../docs/spec/archive/2026-05-12-add-wasm-tests/)）。浏览器装到 `artifacts/tools/playwright-browsers/`（~280MB，gitignored），不污染系统。

## API 概览

完整 TS 类型见 [`js/index.d.ts`](js/index.d.ts)：

```ts
import init, { Z42VM, readNamespaces } from '@z42/wasm';
import { bundleStdlibNode } from '@z42/wasm/stdlib-resolver';

await init();  // 浏览器需要 await fetch；Node 路径自动从 fs 读

const vm = new Z42VM({
    zpkgResolver: await bundleStdlibNode(readNamespaces),
    stdoutHandler: (bytes) => process.stdout.write(bytes),
});
const module = vm.loadZbc(zbcBytes);
const entry  = vm.resolveEntry(module, 'My.Namespace.Main');
vm.invoke(entry);
vm.dispose();
```

### `Z42VMOptions`

| 字段 | 形态 | 用途 |
|------|------|------|
| `zpkgResolver` | 函数 `(name) => Uint8Array \| null` 或对象 `{ resolve(name) }` | 把 namespace 解析成 zpkg 字节。runtime 总是先问 resolver，miss 后才考虑 search_paths（wasm 没有 search_paths） |
| `stdoutHandler` | `(bytes: Uint8Array) => void` | 接 `Console.WriteLine` 输出。每次写一调一次 |
| `stderrHandler` | 同上 | 接 `Console.Error.WriteLine` |

### 内置 resolver helpers

`@z42/wasm/stdlib-resolver` 提供两套工具（都需传入 wasm 的 `readNamespaces` 导出——用它读各 zpkg 的 `NSPC` 建 namespace→bytes 表，不再依赖 `index.json`）：

- `bundleStdlibNode(readNamespaces)` — Node 从包内 `stdlib/` 枚举并加载所有 stdlib zpkg
- `bundleStdlibBrowser(baseUrl, readNamespaces)` — 浏览器 fetch build 生成的 `files.json` + 各 zpkg

两套都返回 `(name) => Uint8Array | null` 形态的 resolver。

## 目录结构

```
wasm/
├── Cargo.toml               wasm crate (cdylib + wasm-bindgen)
├── src/                     Rust 桥
│   ├── lib.rs               Z42VM #[wasm_bindgen] 入口
│   ├── value.rs             Z42VMValue ↔ JsValue marshal
│   ├── error.rs             HostError → JsValue 含 name + status
│   └── resolver.rs          JsCallbackResolver bridge
├── js/                      npm package surface
│   ├── package.json         @z42/wasm
│   ├── index.{js,d.ts}      入口 + TS 类型
│   ├── stdlib-resolver.js   bundleStdlibNode / bundleStdlibBrowser
│   ├── stdlib/*.zpkg        ← `test platform wasm assets` 从 stdlib dist 复制
│   ├── pkg-web/             ← wasm-pack --target web 产物
│   └── pkg-nodejs/          ← wasm-pack --target nodejs 产物
├── demo/
│   ├── fixtures/hello.z42   demo 源代码（assets 编 → js/fixtures/hello.zbc）
│   ├── node/run.js          Node hello-world demo（pkg-nodejs smoke）
│   └── web/                 浏览器版 hello-world demo（无 Node，配静态服务器）
│       ├── index.html
│       └── run.js
└── tests/                   Playwright R1–R7（`test platform wasm run`）
```

`pkg-*/` 和 `stdlib/` 由 `./xtask test platform wasm build` / `assets` 重新生成；都在 `.gitignore` 内。
（构建逻辑已从 `build.sh`/`test.sh` 迁入 `WasmBackend`，见 [`scripts/xtask_test_wasm.z42`](../../../../scripts/xtask_test_wasm.z42)。）

## 限制（v0.1）

- **仅 interp 模式**：wasm 沙箱禁动态代码生成；JIT / AOT 不可用
- **无文件系统**：必须通过 `zpkgResolver` 把 zpkg 字节喂进来；`search_paths` 在 wasm 上无效
- **同步 invoke**：v0.1 不支持 async；长任务会阻塞 JS 主线程，需要用户自行 `Worker`
- **marshal**：JS ↔ z42 仅支持 null / boolean / number / bigint。string / object / Array 推迟到后续 spec（见 [`embedding.md §12 Deferred`](../../../../docs/design/runtime/embedding.md)）
- **单实例**：与桌面 / iOS / Android 一致 — 一个进程内一个 Z42VM；多实例进 Deferred

## 故障排查

| 现象 | 可能原因 | 处理 |
|------|----------|------|
| `wasm-pack not found` | 工具链未装 | `cargo install wasm-pack --locked` |
| `wasm32-unknown-unknown target not installed` | rust target 缺 | `rustup target add wasm32-unknown-unknown` |
| `fixture missing: hello.zbc` | 编译器/stdlib 没构建 | 先 `dotnet build src/compiler/z42.slnx` + `./xtask build stdlib`，再 `./xtask test platform wasm assets` |
| `Z42VMError: undefined function Std.IO.Console.WriteLine` | stdlib zpkg 没载入 | 检查 `js/stdlib/*.zpkg` 是否生成（`test platform wasm assets` 末尾列出复制数）|
| Node demo 报 `command not found: node` | Node 未装 | `brew install node`（或任何 Node ≥ 18 发行版）|

## 与跨平台契约的对齐

本 facade 严格遵守 [`platform-contract.md`](../README.md) 的同形 API + 命名约定：

- 类名 `Z42VM` / `Z42VMModule` / `Z42VMEntry` / `Z42VMValue` / `Z42VMError`（跨平台一致）
- `ZpkgResolver` 协议：函数或 `{ resolve }` 对象任一
- 错误码 → `Z42VMError.status` 映射详见 platform-contract.md §错误码映射表

## 下一步

H4 内剩余两个平台 — `add-platform-ios` / `add-platform-android` — 解锁。两者直接复用本 wasm spec 顺手解决的 `native-interop` feature gate。
