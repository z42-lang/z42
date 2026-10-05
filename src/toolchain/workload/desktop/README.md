# workload/desktop — 桌面平台（apphost stub + Tier-1 C ABI 测试）

> 与 [`../wasm`](../wasm/) / [`../ios`](../ios/) /
> [`../android`](../android/) 平级：desktop 作为统一 `test platform` 框架的第 4 个平台。

## 职责

桌面(host)平台的 **R1–R7 嵌入契约**测试——一个真实外部 C 消费者链接 `libz42.a`，
经 Tier-1 C ABI（[`z42_host.h`](../../../runtime/include/z42_host.h)）跑与 wasm/iOS/Android
facade **同一套 7 场景**。覆盖桌面 C-ABI 这条路径的「链接 libz42.a 的外部程序」端到端。
另含 per-RID apphost stub 的 Rust 源（`platform/apphost/`）。

不做：
- 面向用户的 C / Rust 嵌入示例（由学习手册嵌入章节提供）

## 如何测试验证

```bash
./xtask test platform desktop          # ①libz42.a ②fixtures ③cc+跑+junit
./xtask test platform desktop build    # 只 ① cargo rustc staticlib
./xtask test platform desktop run      # 只 ③ cc r1_r7.c + 跑
```

后端实现 [`scripts/test/xtask_test_desktop.z42`](../../../../scripts/test/xtask_test_desktop.z42)
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
