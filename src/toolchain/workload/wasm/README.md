# workload/wasm — @z42/wasm WebAssembly facade

## 职责

把 z42 VM 包成 WebAssembly + JS facade（`@z42/wasm`），供浏览器 / Node.js / wasm runtime 跑 `.zbc`；
并含 wasm 平台 workload 的 appbuilder（`export`）、测试宿主与 R1–R7 契约测试。仅 interp，无文件系统。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| `Z42VM` wasm-bindgen 入口 | `platform/src/lib.rs` |
| 值 / 错误 / resolver 的 JS 桥 | `platform/src/{value,error,resolver}.rs` |
| JS 入口 + TS 类型 | `platform/js/index.js`、`platform/js/index.d.ts` |
| stdlib resolver（`bundleStdlibNode` / `bundleStdlibBrowser`） | `platform/js/stdlib-resolver.js` |
| 发布 / 导出管线（`: WorkloadBase`） | `appbuilder/WasmWorkload.z42`、`appbuilder/export.z42` |
| 嵌入测试宿主 | `testhost/{index.html,run.js}` |
| Playwright R1–R7 + 嵌入语料 | `platform/tests/` |

## 基础用法

```ts
import init, { Z42VM, readNamespaces } from '@z42/wasm';
import { bundleStdlibNode } from '@z42/wasm/stdlib-resolver';

await init();
const vm = new Z42VM({
    zpkgResolver: await bundleStdlibNode(readNamespaces),   // 经各 zpkg 的 NSPC 建 namespace→bytes 表
    stdoutHandler: (bytes) => process.stdout.write(bytes),
});
vm.invoke(vm.resolveEntry(vm.loadZbc(zbcBytes), 'My.Namespace.Main'));
vm.dispose();
```

`Z42VMOptions`：`zpkgResolver`（函数或 `{ resolve }`）、`stdoutHandler`、`stderrHandler`；完整类型见 `platform/js/index.d.ts`。
浏览器侧 `bundleStdlibBrowser(baseUrl, readNamespaces)` 读构建生成的 `files.json`（zpkg 文件名列表）。

构建与 demo：

```bash
rustup target add wasm32-unknown-unknown && cargo install wasm-pack --locked
./xtask build stdlib
./xtask test platform wasm build      # wasm-pack web + nodejs
./xtask test platform wasm assets     # fixtures + stdlib + files.json
./xtask deps install --os wasm        # 本地缺 Node 时（装到 artifacts/tools/node）
node src/toolchain/workload/wasm/platform/demo/node/run.js     # 期望 [host] hello, world
```

浏览器 demo：`platform/demo/web/index.html`，配任一静态服务器。

限制：仅 interp；同步 invoke；marshal 仅 null / boolean / number / bigint；单实例；
`pkg-*/`、`js/stdlib/` 等生成物落宿主工程副本 `artifacts/build/toolchain/workload/wasm/tests/host/`，不写本目录。

## 如何测试验证

```bash
./xtask test platform wasm
```

build + assets + pkg-nodejs Node smoke + headless chromium 的 Playwright R1–R7，期望 `7 passed`。
浏览器装到 `artifacts/tools/playwright-browsers/`。后端实现 [`scripts/test/xtask_test_wasm.z42`](../../../../scripts/test/xtask_test_wasm.z42)。

## 关联文档

- 跨平台契约：[`../platform-contract.md`](../platform-contract.md)
- 嵌入机制：[embedding.md](../../../../docs/internals/src/runtime/embedding.md) §6.2 / §11
- 构建与故障排查：[build-platforms.md](../../../../docs/internals/src/devinfra/build-platforms.md)

## 核心文件

| 路径 | 职责 |
|------|------|
| `appbuilder/` | workload handler + export |
| `template/` | `export` 渲染进用户工程的脚手架 |
| `platform/Cargo.toml` + `src/` | wasm crate（cdylib + wasm-bindgen） |
| `platform/js/` | npm package surface（`@z42/wasm`） |
| `platform/demo/` | Node / 浏览器 hello-world demo |
| `platform/tests/` | Playwright 测试 |
| `testhost/` | 嵌入测试宿主 |
