# WASM facade — 嵌入 z42 到 WebAssembly

> 🟢 已落地 · facade [`wasm/platform/`](../../../src/toolchain/workload/wasm/platform/) · spec [`2026-05-12-add-platform-wasm/`](../../spec/archive/2026-05-12-add-platform-wasm/)

把 z42 VM 编进 WebAssembly，让 JS / TS 宿主 `import` 跑 `.zbc`。统一三段：**① Host 环境准备 → ② 编译（facade + 嵌入 app）→ ③ 运行测试用例**。

## 1. Host 环境准备

### 1.1 平台工具链（一次性）

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```

❗ Rust 1.88+ 必须加 `--locked`，否则 `cargo-platform` 版本冲突。

可选 demo 工具（仅 §3 跑 demo 时需要）：

- **浏览器 demo**（推荐）：需要一个能正确发送 `application/wasm` / `text/javascript` MIME 的静态服务器。二选一：
  - **Rust（默认）** — 与 z42 toolchain 一致：
    ```bash
    cargo install miniserve --locked
    ```
  - **Python 3** — 多数 macOS / Linux 开发机自带，零安装：
    ```bash
    python3 -m http.server 8000     # 自带正确 .wasm MIME（Python 3.7+）
    ```
  其他可行替代（自行选用，本文档不展开）：`npx serve`、`caddy file-server`、`busybox httpd` 等。
- **Node demo**：
  ```bash
  brew install node          # macOS；其他平台用任意 ≥ 18 发行版
  ```

### 1.2 z42 工具链（编译器 + stdlib，一次性 / 改 stdlib 后重跑）

facade 会把 stdlib zpkg 收进 `js/stdlib/`，故先备好 z42 工具链：

```bash
./xtask build compiler      # z42c 自举（或由 ./scripts/install-z42.sh 直接提供）
./xtask build stdlib
```

✅ 产出 `artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg` + stdlib zpkg 到 `artifacts/build/libraries/dist/release/*.zpkg`。
❗ `error: z42c not built` → 先 `./scripts/install-z42.sh` 或 `./xtask build compiler`。

## 2. 编译

### 2.1 编 facade

```bash
./xtask test platform wasm build     # wasm-pack web+nodejs → pkg-web/ pkg-nodejs/
./xtask test platform wasm assets    # fixtures + stdlib + files.json
```

✅ 末尾应看到：

```
built:
  .../pkg-web/
  .../pkg-nodejs/
  .../js/stdlib/
```

❗ `wasm-pack: command not found` → 1.1 漏装。
❗ `fixture missing: hello.zbc` 或 `stdlib libs dir not found` → 1.2 漏跑。

### 2.2 嵌入到 app

`pkg-web/`（浏览器）/ `pkg-nodejs/`（Node）是标准 wasm-bindgen npm 包；JS / TS 宿主 `import` 后加载 `.zbc` + `js/stdlib/` 跑。完整 JS / TS API + 错误码见 [`wasm/README.md`](../../../src/toolchain/workload/wasm/README.md)；可运行的最小示例见 §3 的 demo。

## 3. 运行测试用例

### 3.1 R1–R7 嵌入契约（Playwright）

```bash
./xtask deps install --os wasm   # wasm-pack + wasm32 target + 本地 Node LTS（wasm 必备三件套；已装可跳）
# node → artifacts/tools/node；PATH 上已有 ≥ min 版的 node 也可用，两者皆缺时测试步骤自动装

./xtask test platform wasm
# ① wasm-pack build web+nodejs → ② fixtures+stdlib+files.json
# → ③ npm install + playwright install chromium（首次下载 ~280MB）+ R1–R7
```

> JUnit 输出：`artifacts/test-reports/wasm/junit.xml`。完整三阶段见 [`../testing/platform-tests.md`](../testing/platform-tests.md)。

### 3.2 浏览器 demo（可选 · 无需 Node）

在 `src/toolchain/workload/wasm/platform/` 起一个静态服务器，把同目录的 `pkg-web/`、`js/stdlib/`、`demo/` 整个目录暴露出去，浏览器打开 `demo/web/index.html`。

```bash
cd src/toolchain/workload/wasm/platform
miniserve --index demo/web/index.html .          # 默认 8080，根路径直接落 index
# 或：python3 -m http.server 8000                 # 路径 /demo/web/index.html
```

然后浏览器访问 `http://127.0.0.1:<port>/`（miniserve）或 `http://127.0.0.1:<port>/demo/web/index.html`（Python）。

✅ 期望：页面状态条变成 **OK**，日志区出现 `[host] Hello, World!`。

❗ DevTools 看到 `Failed to load module` / MIME 报错 → 服务器没有给 `.wasm` 送 `application/wasm`、或给 `.js` 送 `text/javascript`；换 miniserve / Python 3.7+ 任意一个均可。
❗ `Z42VMError: undefined function ...` → `js/stdlib/` 空；重跑 2.1。
❗ `file://...` 直接打开 index.html 会失败（CORS + 不能 fetch wasm）。**必须** 走 HTTP。

### 3.3 Node demo（可选 · 装了 Node 才用）

```bash
node demo/node/run.js
```

✅ 期望：`[host] Hello, World!`
❗ `Z42VMError: undefined function ...` → `js/stdlib/` 空；重跑 2.1。

## See also

- **本地打 browser-wasm SDK package**（自包含 staticlib + cdylib + wasm-bindgen 双 target + npm `package.json`）：[`../packaging.md`](../packaging.md) — `./xtask package runtime --rid browser-wasm`
- JS / TS API + 错误码：[`wasm/README.md`](../../../src/toolchain/workload/wasm/README.md)
- 跨平台契约：[`platform-contract.md`](../../../src/toolchain/workload/platform-contract.md)
- 设计 + 决策：[spec archive](../../spec/archive/2026-05-12-add-platform-wasm/)
