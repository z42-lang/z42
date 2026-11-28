# workload/desktop — 桌面平台（apphost stub 与 C ABI 测试）

> 与 [`../wasm`](../wasm) / [`../ios`](../ios) /
> [`../android`](../android) 平级：desktop 作为统一 `test app` 框架的第 4 个平台。

## 职责

桌面(host)平台的 **R1–R7 嵌入契约**测试——一个真实外部 C 消费者链接 `libz42.a`，
经统一 C ABI（[`z42.h`](../../../runtime/include/z42.h)）跑与 wasm/iOS/Android
facade **同一套 7 场景**。覆盖桌面 C-ABI 这条路径的「链接 libz42.a 的外部程序」端到端。
另含 per-RID apphost stub 的 Rust 源（`platform/apphost/`）。

不做：
- 面向用户的 C / Rust 嵌入示例（由学习手册嵌入章节提供）

## Runtime 链接

从目标 Runtime 包的 `native/include/` 引入 `z42.h`。归档顺序、平台库及组件初始化入口以包内 `manifest.toml` 为准。

| 平台 | 静态 | 动态 |
|---|---|---|
| macOS | `libz42.a`（另加 `-liconv -lc -lm`） | `libz42.dylib`：install name 是 `@rpath/libz42.dylib`，宿主加 `-Wl,-rpath,<放 dylib 的目录>`（如 `@executable_path`） |
| Linux | `libz42.a`（另加 `-lc -lm -lpthread -ldl -lrt -lgcc_s`） | `libz42.so`：SONAME 是 `libz42.so`，宿主加 `-Wl,-rpath,<目录>`（如 `$ORIGIN`）或走 `LD_LIBRARY_PATH` |
| Windows | `z42.lib` | 链 import lib `z42.dll.lib`，运行时 `z42.dll` 放在 exe 旁 |

组件化动态部署使用同一包的核心、Rust std 和所需组件。将 `native/` 放在宿主可执行文件旁，或用 `Z42_NATIVE_PATH` 指定组件目录。直接链接共享组件时使用包内对应库文件及统一登记接口。Windows 的核心、解释器、JIT、profiler 和 compression 导入库见 `[assets].import_libs`。静态链接所需第三方库见 `[assets].third_party_libs`，链接参数见 `[components.<stem>].link_flags`。

Windows 宿主按运行时装载 DLL 时，使用 `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS` 或通过 `AddDllDirectory` 添加包的 `native/`，使依赖的共享 Rust std 可被解析。

Linux Runtime 的 glibc 最低版本为 x64 2.34、arm64 2.39。C 宿主动态库带 ELF `NODELETE`，保持线程退出时的 TLS 析构回调有效。

组件用法、状态码和生命周期见 [嵌入 C ABI](../../../../docs/reference/src/embedding/c-abi.md)。

## 如何测试验证

```bash
./xtask test app desktop          # ①libz42.a ②fixtures ③cc+跑+junit
./xtask test app desktop build    # 只 ① cargo rustc staticlib
./xtask test app desktop run      # 只 ③ cc r1_r7.c + 跑
```

后端实现 [`scripts/src/test/app/desktop.z42`](../../../../scripts/src/test/app/desktop.z42)
（`DesktopBackend : IPlatformBackend`）。JUnit → `artifacts/reports/tests/desktop/junit.xml`。

## 关联文档

R1–R7 契约见 [`docs/internals/src/testing/cross-platform.md`](../../../../docs/internals/src/testing/cross-platform.md)
的 platform 冒烟契约表（R1 smoke / R2 bad zbc=10 / R3 unknown entry=20 / R4 arg mismatch=21 /
R5 resolver miss / R6 lifecycle / R7 multi-line）。

## 核心文件

| 文件 | 职责 |
|------|------|
| `platform/apphost/` | per-RID apphost stub 的 Rust 源，`z42 publish` 必需 |
| `shell/apphost_embed.c` / `shell/testhost.c` | 嵌入测试壳 C 源 |
| `tests/r1_r7.c` | R1–R7 C harness：`z42_host_*` 跑 7 场景 + 状态码断言；每场景打 `[Rn] PASS/FAIL`，全过 exit 0 |
