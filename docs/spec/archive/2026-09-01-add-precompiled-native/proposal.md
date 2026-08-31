# Proposal: `[native]` 预编译库配置与复制（no-hook committed prebuilt）

> add-native-dep-config 的**姊妹路径 / PR-2**。PR-1 做「hook 现场编 native」，本 change 做「直接指向
> 已编译好的 native 文件」。地基（`[native.<name>]` schema + `_pubBundleProjectNativeDeps` 闭包遍历 +
> `_pubCopyDistDeps` 平铺 + 运行期 `resolve_native_beside`）已由 PR-1（#358，origin/main `4b3c0004`）落地。

## Why

PR-1 的传递复制 `_pubBundleProjectNativeDeps` 目前**只处理带 `[build] hooks` 的 dep**：

```z42
// builder_publish.z42:658
if (_pubHasNative(depToml) && _pubTomlStr(depToml, "build", "hooks", "").Length > 0) {
    _pubRunDepProvideNative(...);   // 跑 dep 的 ProvideNative 现场产出
}
// ↑ 声明 [native] 但**无 hook**（已提交预编译库）的 dep → 当前被静默跳过（PR-1 Deferred）
```

一个包若只是**携带一个已经编译好的 native 库文件**（.so/.dylib/.dll，来源/语言无关——rust/c/c++/vendor
blob 都行），不想每次 publish 现场编（无 cargo/cc、或纯二进制 vendor blob），今天**无路可走**：它的
native 不会被消费方复制进 payload。User 明确要补这条（有需求驱动，非投机）。

目标：让 `[native.<name>]` 能**指向一个已提交的预编译 native 文件**，消费方 publish 时**按目标 rid**
把它平铺进 payload——PR-1「hook 现场编」的对称面。**只做这一件事**，不碰 cross-compile / single-file /
显式任意命名 vendor blob 等其它 Deferred。

## What Changes

- **schema 扩展**：`[native.<name>]` 加可选键 **`dir`**——指向已提交预编译库的基目录（相对 manifest），
  文件按既有派生约定落在 `<dir>/<rid>/<prefix><name><suffix>`（`<prefix>`=`DLL_PREFIX`：Windows 空、其他
  `lib`；`<suffix>`=`.dylib`/`.so`/`.dll`）。`dir` 存在且**无 `[build] hooks`** → 预编译路径；有 hooks →
  PR-1 hook 路径（不变，hooks 优先）。
- **模型**：`NativeSpec` 加 `string Dir`（空=无）；`ManifestLoader._parseNative` 读每张 `[native.<name>]`
  子表的 `dir` 键。
- **消费侧 no-hook 分支**：`_pubBundleProjectNativeDeps` 对声明 `[native]` 但无 hooks 的 dep 走新
  `_pubCopyPrebuiltNative`——从 `<depDir>/<dir>/<目标rid>/<派生文件名>` 定位预编译文件 → 平铺进消费方
  distDir（再由已有 `_pubCopyDistDeps` 带进 payload，`_pubIsNativeLib` 已认 .so/.dylib/.dll）。
- **rid 派生文件名**：新 `_pubNativeFileName(name, rid)`——按**目标 rid**（非 host）派生 `<prefix><name><suffix>`
  （镜像 `_familyOfRid`）。这与 PR-1 repl hook 的 host-based `_nativeFileName` 有意不同：预编译库要能
  **交叉目标**（macOS 上 publish linux-x64 取 `libfoo.so`），故按目标 rid 派生。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.project/src/NativeSpec.z42` | MODIFY | 加 `string Dir`（受限子集：sealed class + 构造函数） |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | `_parseNative` 读每张 `[native.<name>]` 子表 `dir` 键 |
| `src/libraries/z42.project/tests/manifest_native.z42` | MODIFY | 加 `dir` 存在/缺省两场景断言 |
| `src/toolchain/builder/core/builder_publish.z42` | MODIFY | `_pubBundleProjectNativeDeps` 加 no-hook 分支；新 `_pubCopyPrebuiltNative` + `_pubNativeFileName` |
| `scripts/test/xtask_test_dist.z42` | MODIFY | `_apphostSmoke` 加「path-dep 携带预编译 native → publish → 断言平铺进 payload」一腿 |
| `docs/book/src/runtime/native-libraries.md` | MODIFY | §3 把「无 hook 的 committed 预编译库消费路径」从 Deferred 移为实况 + `dir` schema |
| `src/libraries/z42.project/README.md` | MODIFY | NativeSpec 增加 `Dir` 字段说明（若功能索引提及） |

**只读引用**（不改，仅参照）：
- `src/toolchain/builder/core/builder.z42` `_familyOfRid`（rid→family 派生规则参照）
- `src/toolchain/interactive/repl/hooks/hooks.z42` `_nativeFileName`（host-based 版对照）
- `src/runtime/src/native/ext.rs` `resolve_native_beside`（运行期契约不变）

## Out of Scope

- **显式 per-rid 任意路径**（`files."<rid>" = "<path>"`，破 `<dir>/<rid>/<派生名>` 约定的 vendor blob）→
  仍 Deferred（design Decision 的逃生口），等真实非常规命名消费者。
- **hook 现场编** → PR-1 已做，本 change 不碰。
- **cross-desktop / 移动端 native 交叉编译产出** → 仍 Deferred；本 change 只做「复制已提交文件」，复制本身
  天然支持任意目标 rid（有对应 `<rid>/` 子目录即可），但不负责**产出**跨目标二进制。
- **`[native.dependencies]` app 侧声明外部预编译库** → 同族后续（本 change 仍是「本包**提供** native」面）。

## Open Questions（DRAFT 待 User 6.5 裁决）

- [ ] **Q1 — `dir` 布局是否含 `<rid>/` 子目录？** 推荐 **含**（`<dir>/<rid>/<派生名>`，与 dist 的 `<rid>/`
      约定一致、交叉目标就绪）。备选：`<dir>/<派生名>`（无 rid 子目录，单目标最简但无法多 rid 共存）。
- [ ] **Q2 — 测试策略。** 推荐：①parse 单测（cheap，走 `xtask test stdlib z42.project`）+ ②在
      `_apphostSmoke`（test dist，已驱动真 `z42 publish`）加一腿合成 fixture（写假 native 到
      `prebuilt/<rid>/`，publish 后断言平铺进 payload）。②需打包 SDK 才跑（无包则 SKIP）→ 本地
      `xtask package sdk && xtask test dist` 或交 CI。备选：只做 parse 单测（放弃 e2e，因无真实预编译
      消费者、builder 无单测 harness）。
- [ ] **Q3 — 单 PR vs support/use 两 PR？** 倾向**单 PR**：无新语法/新 zbc·zpkg 格式；`dir` 是 publish 期
      消费的数据；`NativeSpec.Dir` / `_pubCopyPrebuiltNative` 均由**当前 z42c** 编（z42.project/builder 是
      stdlib/toolchain，非 seed 编的 xtask·z42c 源）。实施第一步 grep 确认 seed 编的 xtask/z42c 源不读
      `.Dir` → 落单 PR。
