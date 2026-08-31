# Spec: `[native]` 预编译库配置与复制

> 扩展 add-native-dep-config 的 native-dep-declaration spec。仅规定**新增/变更**的可观察行为。

## R1 — schema：`[native.<name>].dir`

- **R1.1** `[native.<name>]` 子表可含可选字符串键 `dir`，值为**相对 manifest 目录**的基目录路径。
- **R1.2** `dir` 缺省 = 空串（无预编译指向）。
- **R1.3** 声明预编译库的文件按既有派生约定落在 `<manifest-dir>/<dir>/<rid>/<prefix><name><suffix>`：
  - `<prefix>` = `DLL_PREFIX`：`windows-*` 空、其它 `lib`；
  - `<suffix>` = `.dll`（`windows-*`）/ `.dylib`（`macos-*`/`ios-*`/`iossim-*`）/ `.so`（其它，linux/android）。
- **R1.4** 解析层：`ManifestLoader._parseNative` 把每张 `[native.<name>]` 的 `dir` 填入
  `ProjectManifest.Natives[i].Dir`；名按稳定序发射（common-pitfalls §1，与 PR-1 一致）。

## R2 — publish 消费：no-hook 预编译分支

- **R2.1** `z42b publish <exe> --rid R` 遍历 exe 的 path-dep 闭包时，对每个声明 `[native]` 的 dep：
  - dep **有** `[build] hooks` → 跑 `ProvideNative`（PR-1 行为，不变，hooks 优先）；
  - dep **无** `[build] hooks` → 走预编译路径（R2.2）。
- **R2.2** 预编译路径：对 dep 每个 `[native.<name>]`，若其 `dir` 非空，从
  `<dep-manifest-dir>/<dir>/<R>/<prefix><name><suffix>`（R 为目标 rid，R1.3 派生）定位文件，**平铺**（去
  `<rid>/` 一层）复制进消费方 distDir。随后既有 `_pubCopyDistDeps` 把它带进 payload。
- **R2.3** 文件名按**目标 rid R** 派生（非 host）——交叉目标时取 R 对应的 prefix/suffix。
- **R2.4** best-effort：`dir` 为空（无 hooks 又无 dir）或预编译文件不存在 → **警告并跳过**该 native，不
  中断整个 publish（与 PR-1 `_pubRunDepProvideNative` warn-not-abort 一致）。
- **R2.5** 运行期 `resolve_native_beside` 契约不变：payload 中 `<prefix><name><suffix>` 挨着消费方 zpkg
  即可被按名解析。
- **R2.6**（Decision 5）native 传递复制的**闭包遍历按 dep 声明的 `{ path }` 解析** dep manifest（相对声明
  方目录）；仅 version-string / 无 path 的 dep 回落到仓内 name-search。**不再**在 `srcRoot==""`（仓外消费者）
  时提前退出——path-dep 的 native 对仓外消费者同样复制。无 path 且仓外无法定位的 dep → 保守跳过（不阻断）。

## R3 — 边界

- **R3.1** dep 同时有 `[build] hooks` 与 `[native.<name>].dir` → hook 优先，`dir` 忽略（R2.1）。
- **R3.2** 复制只做「已存在文件」的搬运——本 change 不产出任何二进制（不 cross-compile）。目标 rid 无对应
  `<rid>/` 文件即 R2.4 跳过。

## 验收

- **AC1（parse）**：`[native.foo]\ndir = "prebuilt"` → `Natives[0].Name=="foo"` 且 `Natives[0].Dir=="prebuilt"`；
  无 `dir` → `Dir==""`（`manifest_native.z42` 单测）。
- **AC2（rid 派生）**：`_pubNativeFileName("foo","windows-x64")=="foo.dll"`、`"macos-arm64"→"libfoo.dylib"`、
  `"linux-x64"→"libfoo.so"`（e2e 内隐式覆盖 host rid；跨 rid 派生由 code review + 断言表覆盖）。
- **AC3（e2e）**：合成 lib（`[native.pnfoo] dir="prebuilt"` + 写入假 `prebuilt/<hostRid>/<派生名>`）+ 一个
  path-dep 它的 exe → `z42 publish` → payload 中出现 `<派生名>`（`_apphostSmoke` 新一腿；无包时 SKIP）。
