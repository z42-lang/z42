# Tasks: zpkg 版本失配不再静默跳过

> 状态：🟢 已完成（2026-09-22）｜ scope: stdlib + compiler + docs（无格式 bump）

## 进度概览

| # | 阶段 | 状态 |
|---|---|---|
| 0 | 调研（两个 reader / 调用点是否持有路径 / VM 侧措辞） | 🟢 已完成 |
| 1 | `Open(byte[], string origin)` 重载 + `_warnVersionSkew` + 按版本去重 | 🟢 已完成 |
| 2 | 6 个调用点传路径 | 🟢 已完成 |
| 3 | 端到端实证 + 退回对照 | 🟢 已完成 |
| 4 | 回归测试 + 判别力验证 | 🟢 已完成 |
| 5 | 文档（zpkg.md「有两个 reader」） | 🟢 已完成 |
| 6 | 完整 GREEN + 归档 | 🟢 已完成 |

## 阶段 0 —— 调研（勿重跑）

- [x] 0.1 `ZpkgReader.Open` 只收 `byte[]`，**拿不到路径**——而路径正是最该说出来的那半。
      7 个调用点里 **6 个**手上就有路径（5 处字面写着 `File.ReadAllBytes(<path>)`），
      剩下一个（`DepScan.ExtendWithPackage`，REPL 内存包）有 `pkgName`。⇒ 加 `origin` 重载。
- [x] 0.2 `ConsoleError` 在 `Std.IO`（z42.core），`z42.build` / `z42.cli` / `z42.net` 都有
      库往 stderr 写的先例。
- [x] 0.3 措辞对照 Rust 侧 `app.rs::version_mismatch_hint`（在检测点点名 + 给补救命令）。
- [x] 0.4 **发现文档早于实现**：`zpkg.md` 文件头一节已写着 reader strict-pin「**不再静默跳过**」——
      那说的是 **Rust** 那个 reader；z42 侧这个一直在静默。两个 reader、一条政策、只落地了一半。

## 阶段 3 —— 端到端实证（新告警零触发 = 零证据）

复刻真实场景：把一个真包的 minor 从 49 改成 43，放进 `Z42_LIBS` 指向的目录再编译。

```
$ printf '\x2b\x00' | dd of=/tmp/zw-libs/z42.crypto.zpkg bs=1 seek=6 conv=notrunc
$ Z42_LIBS=/tmp/zw-libs z42c.driver -- build /tmp/zw-proj/hello.z42.toml --release
warning: skipping `/tmp/zw-libs/z42.crypto.zpkg` — it was built for z42 package format 0.43,
  this toolchain reads 0.49 (strict-pin: there is no cross-version compatibility).
  A skipped package is invisible to dependency resolution, so this usually resurfaces far
  from here as bogus `undefined: <Type>` / `undefined function` errors.
  Rebuild it against this toolchain (`xtask build stdlib`), or run the toolchain that
  produced it. Further packages built for 0.43 are skipped without repeating this.
```

- [x] 3.1 实证告警真打出来、带路径、带因果、带修法。
- [x] 3.2 **退回对照**：同场景用 main 版 z42c + main 版 libs → 编译期**一声不吭**（只有 VM 那条
      运行期 WARN）。⚠️ 第一次对照做**砸了**：只换了 driver 没换 libs，结果它从同一个
      `Z42_LIBS` 里加载了**我改过的 `z42.ir`**，照样打出我的告警 —— 差点据此得出「main 也会报」
      的错误结论。**对照组必须两侧同源**（同 [[verify-conclusion-after-reseeding]]）。
      副产品：那次误跑反而证明了重载两边都能走通（旧调用点走 1 参 ⇒ `origin` 为空 ⇒ 文案退化成
      "a .zpkg"，不崩）。

## 阶段 4 —— 回归测试

- [x] 4.1 `ZpkgReader.SkewWarningCount()`（去重后的旧版本条数）——stderr 文本抓不到，
      用计数器作证「**报过了**」。
- [x] 4.2 `zpkg.z42` 两条：同版本只报一次 / 不同版本各一条 / 版本对得上不报。
- [x] 4.3 **判别力**：摘掉 `_warnVersionSkew` 调用 → `version_skew_is_reported_once_per_version`
      变红（`values not equal`），还原即绿。

## 阶段 6 —— GREEN

- [x] 6.1 完整 `xtask test` 全绿（base `18a64904e`）。含 `seed cold-start` stage —— 它正好压
      「`z42.ir` 新增方法、同 PR 被 `z42c.pipeline` 调用」这条一层跨成员新符号的路径，未红
      ⇒ proposal 里登记的那条风险不成立，不必回落到「不带 origin 的版本」。
