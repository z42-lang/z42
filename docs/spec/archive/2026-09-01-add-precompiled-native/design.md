# Design: `[native]` 预编译库配置与复制

> 承接 add-native-dep-config（PR-1，#358）。本 change 扩展其 `[native]` schema 与
> `_pubBundleProjectNativeDeps`，补「无 hook、直接指向已提交预编译文件」的对称路径。
> 复用 PR-1 落的：闭包遍历、`_pubCopyDistDeps` 平铺、`_pubIsNativeLib`、运行期 `resolve_native_beside`。

## 背景：PR-1 已有的两半，缺的那半

```
声明 [native.<name>]                        ┌─ 有 [build] hooks ─→ ProvideNative 现场编 → dist/<rid>/ ─┐
  （本包携带私有 native）  ──── publish 消费 ─┤                                                          ├─→ 平铺进 payload
                                            └─ 无 hooks ─────────→ ✗ 当前静默跳过（本 change 补）──────┘
```

PR-1 的 `_pubBundleProjectNativeDeps`（builder_publish.z42:641）遍历消费方 path-dep 闭包，对每个声明
`[native]` 的 dep：**仅当它有 `[build] hooks`** 才跑 `_pubRunDepProvideNative`。无 hooks 的 dep（已提交
预编译库）当前被跳过（PR-1 显式 Deferred：`native-config-future-explicit-files` 逃生口的一部分）。

## Decisions

### Decision 1: 预编译文件定位 —— `dir` 基目录 + 既有 `<rid>/<派生名>` 约定

**问题**：无 hook 时，native 文件在哪？hook 路径把它产到 `dep-dist/<rid>/<派生名>`；预编译库是**已提交
在源码树**里的文件，不在 dist。需要一个指针。

**选项**（承 PR-1 design Decision 1 的 Deferred 逃生口 `native-config-future-explicit-files`）：
- **A（约定式 `dir`）**：`[native.<name>] dir = "<基目录>"`，文件在 `<manifest-dir>/<dir>/<rid>/<prefix><name><suffix>`。
  一行覆盖全平台/全 rid，与 hook 产出的 `dist/<rid>/` 布局同构。
- **B（显式 per-rid map）**：`[native.<name>] files."<rid>" = "<path>"`，任意文件名/布局。最通用但冗长、易漏
  平台、破派生约定。

**决定：选 A**，B 仍留 Deferred。理由：
- A 与 PR-1 已确立的「rid 定子目录、prefix/suffix 平台派生」**同一条约定**——config、hook 产出、预编译
  三者共用 `<rid>/<prefix><name><suffix>`，`resolve_native_beside` 契约不变。
- 预编译库只要按约定放好即可（语言无关）。任意命名 vendor blob（B）属投机，无真实消费者 → Deferred
  （memory 纪律）。

**schema**：
```toml
[native.foo]
dir = "prebuilt"
# 本包携带预编译 native 库 foo；文件在 <manifest-dir>/prebuilt/<rid>/<prefix>foo<suffix>
#   <prefix> = DLL_PREFIX（Windows 空、其他 lib）；<suffix> = .dylib/.so/.dll
# 无 [build] hooks → 视为已提交预编译文件；消费方 publish 按目标 rid 复制。
```

**Q1（待 User）：`dir` 下是否含 `<rid>/` 子目录？** 推荐**含**——交叉目标就绪、与 dist 同构。示例布局：
```
mylib/
  mylib.z42.toml            # [native.foo] dir = "prebuilt"
  prebuilt/
    macos-arm64/libfoo.dylib
    linux-x64/libfoo.so
    windows-x64/foo.dll
```

### Decision 2: rid 派生文件名（非 host 派生）—— 交叉目标正确性

**问题**：PR-1 repl hook 的 `_nativeFileName` 用 `Platform.IsMacOS()` 等 **host** 判定——因 repl host-only。
预编译库的价值恰在**可交叉目标**：macOS 上 publish `--rid linux-x64` 应取 `libfoo.so`，不是 `libfoo.dylib`。

**决定**：publish 侧新 `_pubNativeFileName(name, rid)` 按**目标 rid** 派生，镜像 `builder.z42:_familyOfRid`：

```z42
string _pubNativeFileName(string name, string rid) {
    if (rid.StartsWith("windows-")) { return name + ".dll"; }              // 空 prefix
    if (rid.StartsWith("macos-") || rid.StartsWith("ios-")
        || rid.StartsWith("iossim-")) { return "lib" + name + ".dylib"; }
    return "lib" + name + ".so";                                           // linux / android
}
```

与 Rust `resolve_native_beside`（`DLL_PREFIX`/`DLL_SUFFIX`）**同语义**，只是驱动源从「host consts」换成
「目标 rid 族」。**事实校正记录**：PR-1 host-based 版对 host-only 的 repl 正确、不改；本 change 不复用它，
另立 rid-based 版——两者语义一致（同一派生规则），差别仅「谁定平台」（host vs 目标 rid）。

### Decision 3: 消费侧 no-hook 分支 —— 最小改 `_pubBundleProjectNativeDeps`

`_pubBundleProjectNativeDeps` 的 hook 判定（L658）由「and hooks」改为「有 native 就处理，按 hooks 有无分流」：

```z42
if (_pubHasNative(depToml)) {
    int rc;
    if (_pubTomlStr(depToml, "build", "hooks", "").Length > 0) {
        rc = _pubRunDepProvideNative(depTomlPath, depToml, name, distDir, rid);   // PR-1 不变
    } else {
        rc = _pubCopyPrebuiltNative(depTomlPath, depToml, distDir, rid);          // 本 change
    }
    if (rc != 0) { return rc; }
}
```

`_pubCopyPrebuiltNative`（NEW）—— best-effort（与 PR-1 warn-not-abort 一致）：
```
depDir = dirname(depTomlPath)
for name in sorted(depToml.native.Keys()):              // 稳定名序（common-pitfalls §1）
    dir = depToml.native[name].dir  (默认 "")
    if dir == "": warn("native '<name>' 无 hooks 也无 dir — 跳过"); continue
    fname = _pubNativeFileName(name, rid)
    src   = depDir / dir / rid / fname
    if !exists(src): warn("预编译 native 缺失: <src>（rid=<rid>）"); continue
    dst = distDir / fname                               // 平铺（去 <rid>/），同 _pubRunDepProvideNative
    if src != dst && !exists(dst): copy(src, dst)
return 0
```
平铺进 `distDir` 后，既有 `_pubCopyDistDeps`（`_pubIsNativeLib` 认 .so/.dylib/.dll）把它带进 payload——
**与 hook 路径汇流到同一下游**，无需改 payload 复制。

**hooks 优先**：dep 同时有 `[build] hooks` 与 `dir` → 走 hook 分支，`dir` 被忽略（现场产出优先于预编译）。
文档写明；正常不会二者并存。

### Decision 5: path-dep 按 `{ path }` 解析（去 srcRoot bail）—— 让传递复制对仓外消费者生效

**事实校正（2026-09-01，实施中发现）**：PR-1 的 `_pubBundleProjectNativeDeps` 用 `_pubLocateDepToml(srcRoot,
name)` **按名在仓源码树里搜** dep，且 `srcRoot=="" → return 0` 提前退出。它对 `z42.interactive→z42.repl`
生效**仅因** z42.repl 在仓树内、可按名定位——即便该 dep 声明为 path-dep `{ path="../repl" }`，`{ path }`
被忽略、按名重找。**后果**：任何**仓外消费者**（真实用例：用户项目 path-dep 一个携带预编译 native 的
sibling）`_pubSrcRoot=="" → 静默 no-op`，native 永不复制。故「config + 复制」若不解此，对真实消费者不生效，
且自包含 tempdir e2e 无法覆盖（`xtask test dist` 与任何 tempdir 的 srcRoot 均为 ""）。

**决定（User 6.5 批准）**：`_pubBundleProjectNativeDeps` 改为**按 dep 声明的 `{ path }` 解析** dep toml
（相对声明方 manifest 目录），仅对 version-string / 无 path 的 dep 回落到 `_pubLocateDepToml(srcRoot,name)`
（仓内，不变）；**去掉 `srcRoot=="" → return 0`**（path-dep 仍可解析）。BFS 队列改带 `(name, tomlPath)`，
每个 dep 的 path-dep 相对**它自己**的目录解析（传递正确）。

```z42
string _pubResolveDepToml(parentTomlPath, parentToml, depName, srcRoot):
    path = parentToml.dependencies[depName].path (或 "")
    if path != "": cand = <dirname(parentTomlPath)>/<path>/<depName>.z42.toml; return exists?cand:""
    if srcRoot != "": return _pubLocateDepToml(srcRoot, depName)   // 仓内 name-search（PR-1 不变）
    return ""
```

**无回归**：dogfood 里 z42.repl 现按 path 先解析（`../repl/z42.repl.z42.toml` 存在）→ 命中同一 toml；
version-string 依赖仍 name-search。**收益**：仓外 path-dep 消费者的 native 复制生效 + tempdir e2e 可覆盖。

### Decision 4: 模型 `NativeSpec.Dir` —— 忠实表达 + 单测覆盖

`NativeSpec` 加 `string Dir`（空=无）；`_parseNative` 读子表 `dir`。**说明**：publish 侧 `_pubCopyPrebuiltNative`
直接读 `TomlValue`（沿用 PR-1 `_pubHasNative` 的 raw-toml 风格，避免 builder 依赖整个 ManifestLoader），
故模型 `NativeSpec.Dir` 的**主要用途是「忠实建模 + parse 层单测锚点」**（与 PR-1 `pm.Natives` 现状一致：
模型完整、可测，publish 另走 raw toml）。两处解析同一 schema，行为须一致（单测钉 parse 层，e2e 钉 publish 层）。

## 数据流（端到端）

```
消费方 exe publish (--rid R)
  └ _pubBundleProjectNativeDeps(exeToml, distDir, R)
      └ BFS path-dep 闭包 → dep "mylib" 声明 [native.foo] dir="prebuilt"，无 hooks
          └ _pubCopyPrebuiltNative:
              src = <mylib-dir>/prebuilt/R/<_pubNativeFileName("foo",R)>
              copy → distDir/lib foo.<suffix>              (平铺)
  └ ... payload 段: _pubCopyDistDeps(distDir → payloadDir)
      └ _pubIsNativeLib("libfoo.so") == true → 复制进 payload
运行期: resolve_native_beside(payloadDir, "foo") → 命中 libfoo.<suffix>   (契约不变)
```

## Deferred / Future Work

### native-prebuilt-explicit-files: 显式 per-rid 任意路径
- **来源**：本 design Decision 1（选 A，B 留逃生口）。
- **触发**：出现带**非常规命名**（非 `<prefix><name><suffix>`）预编译 native 的真实 vendor 库。
- **形态**：`[native.<name>] files."<rid>" = "<path>"` 解析 + 消费。
- **workaround**：把预编译库按约定重命名/软链到 `<dir>/<rid>/<prefix><name><suffix>`。

### native-prebuilt-cross-produce: 跨目标二进制的产出
- 本 change 只**复制**已提交的任意-rid 文件；不负责在 host 上**产出**其它 rid 的二进制（那是 cross-compile，
  仍 Deferred）。用户需自备各 rid 的预编译文件放进 `<dir>/<rid>/`。
