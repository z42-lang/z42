# 格式 bump 期的本地验证与 fixture 重生

> 代码：`src/compiler/z42.package/src/BinaryFormat/`、`src/runtime/src/metadata/`
> 相关：[zbc 格式](../formats/zbc.md) · [zpkg 格式](../formats/zpkg.md) · [CI 拓扑](ci.md) · [测试怎么跑](testing.md)

zbc / zpkg minor bump 后，本地有一个死结：**`cargo build` 出来的 z42vm 是新格式 reader，而本地唯一的种子
（`.z42/` 下的 nightly、或 warm `artifacts/`）还是旧格式 writer**——新 VM 读旧种子 zpkg 直接撞墙：

```
zpkg minor <旧> not supported (writer is at <新>)
```

把种子推进到新格式本需**两代自举**（旧 VM 跑 gen1/gen2），而这条链在本地（尤其 macOS）不可用。
**解法：让 CI 先把新格式工具链建出来，下载回本地 overlay 成种子**——种子与 cargo VM 同为新格式之后，
warm 建 / 测 / fixture 重生全部走通，两代自举彻底不需要。

> zpkg 是可移植字节码——linux 建出的 `z42c.driver.zpkg` 也能在 macOS 的 cargo VM 上跑（CI 的 macOS /
> Windows job 一直这么用），所以只需 linux 一份 artifact。

## 配方

前置：bump 各步已改完源码与版本常量（清单见 [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)）。

### 1. 先推一个 PR，拿工具链 artifact

首轮 CI 里 `compile-toolchain` 应绿——它从当前 PR 源码建出新格式的 z42c + 全 stdlib，上传为
`toolchain-ubuntu-latest`。`test-host` 这一轮预期**红在 committed fixture**（还是旧格式、尚未重生），
属正常；这轮只为拿 artifact。

### 2. 下载 + overlay（保留本树的 `runtime/z42vm`）

```bash
RUN=<compile-toolchain 所在 run-id>          # gh run list --branch <your-branch>
gh run download $RUN -n toolchain-ubuntu-latest -D /tmp/tc
rm -rf artifacts/build/compiler artifacts/build/libraries
cp -R /tmp/tc/artifacts/build/compiler   artifacts/build/
cp -R /tmp/tc/artifacts/build/libraries  artifacts/build/
cp    /tmp/tc/artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zpkg
```

🔴 **`xtask.zpkg` 很可能比 `libraries/` 旧一代——拷完必须重建它。** artifact 里的 `xtask.zpkg` 是流水线
**早期**用**种子 stdlib** 建的，同一个 artifact 里的 `libraries/` 是**末期**的新格式产物，两者差一代。
症状是一个离现场很远的缺符号：

```
Error: uncaught exception: Std.MissingSymbolException:
  undefined function `Z42.Project.ManifestLoader.LoadWorkspace$1$string`
  at Z42Xtask._wsBuildRoot(string)        ← 判据：栈顶在 `Z42Xtask.*`
  at Z42Xtask._ensureSeed(string)            ⇒ 是 xtask 二进制自己缺符号，
  at Z42Xtask._buildCompiler()               **不是**被测代码的问题
```

修法是用刚 overlay 进来的新 z42c 直接编 xtask 源：

```bash
rm -f artifacts/xtask/xtask.zpkg artifacts/xtask/xtask.zsym    # 不删则 publish 不重编
Z42_LIBS="$PWD/artifacts/intermediate/libraries/flat/release" \
  artifacts/build/runtime/release/z42vm \
  artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg \
  -- build scripts/xtask.z42.toml --release
```

⚠️ **不要**手工把某个 `.zpkg` 拷进 `Z42_LIBS` 去「修」依赖——那会弄坏依赖解析，报
「A skipped package is invisible to dependency resolution」外加一片**假的** `undefined: <Type>` /
`undefined function`，离真因更远。

### 3. 让 xtask 用本树的新格式 cargo VM

launcher 默认回落 `.z42/bin/z42vm`（旧种子），不设这个变量会报 `minor <新> not supported (writer is at <旧>)`：

```bash
export Z42_PORTABLE_VM="$PWD/artifacts/build/runtime/release/z42vm"
```

### 4. 在新格式下重建、重生、跑门

```bash
xtask build compiler && xtask build stdlib   # warm，同格式
xtask build test                             # 原地重生 6 个 zbc-format fixture
cargo test --lib                             # committed fixture 现应全过
xtask test                                   # 完整 GREEN，含自举不动点 gen1==gen2
```

**zpkg-format fixture 要手工重生**（`xtask build test` 不含 zpkg）：

```bash
VM=$PWD/artifacts/build/runtime/release/z42vm
Z42C=$(find /tmp/tc -name z42c.driver.zpkg | head -1)
LIBS=$PWD/artifacts/intermediate/libraries/flat/release
# 临时工程：name 用 demo.minimal / demo.multi / demo.indexed（按 fixture 目录名对应），kind=lib
#   packed → --release；indexed → 无 --release（另产散装 source.zbc）
Z42_LIBS="$LIBS" "$VM" "$Z42C" -- build <temp>/demo.minimal.z42.toml --release
cp <temp>/dist/demo.minimal.zpkg src/compiler/z42.package/tests/fixtures/zpkg-format/packed-minimal/source.zpkg
# indexed 另拷 dist/source.zbc → indexed-minimal/source.zbc（散装 + FILE 段 hash 自动同步）
```

`sym-only-sidecar` 没有 Rust 字节读测试 → 保持旧格式不动。各 fixture 目录自带 committed 构建配方
（`[project].pack` 决定 packed / indexed，是否带 `--release` 决定 strip / sidecar），完整重生命令见
[zpkg-format fixture README](../../../../src/compiler/z42.package/tests/fixtures/zpkg-format/README.md)。

### 5. 收尾

重生的 fixture 与收尾 commit 一起 push，PR 转全绿后合并。

## 为什么不用「本地两代自举」

两代自举由 `ci-bootstrap` 的版本差 gate 驱动（种子 minor ≠ 当前 writer minor 时用 nightly 自带的旧 VM
跑 gen1/gen2），是 CI 的常规路径、不需要人工传种子；它在本地的复现成本高于本页 overlay 一条路，
详见 [CI 拓扑](ci.md) 与 [编译器自举](../compiler/self-hosting.md)。
