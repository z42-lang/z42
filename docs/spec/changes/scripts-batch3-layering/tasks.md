# scripts-batch3-layering — 通用原语上移 `common/`，解开三条反向依赖

> 类型：`refactor`（最小化模式，无需 DRAFT 规范）。
> 属 scripts/ 结构优化程序**第 3 批**（… → #485 handler 归位 → #489 命名归位 → 本次）。

## 问题

`scripts/` 的目录分层意图是 **各命令族（`build/` `test/` `package/` `install/` `cli/`）→ `common/`
单向依赖**。但 namespace 扁平（`Z42Xtask`）+ `include = ["**/*.z42"]` 意味着**任何文件都能裸名
调任何文件的函数，编译器不会拦**——于是积了三条反向依赖，全是「某个通用原语碰巧第一次被
用到时写在了哪个族里，就一直留在那儿」：

| 原语 | 定义在 | 谁在反向调它 |
|---|---|---|
| `_copyAll` / `_resetDir` | `build/xtask_compiler.z42` | **`common/xtask_common.z42`** ×3（底层反调上层）、`package/xtask_release.z42` |
| `_linkAll` / `_cleanGlob` | `build/xtask_stdlib.z42` | `test/xtask_test_cross.z42` ×3 |
| `_makeExe` / `_copyIfExists` | `package/xtask_package.z42` | **`build/xtask_stdlib.z42`** ×9 |

后果不是编译错误，是**读代码时在目录间打转**：想知道 `common/xtask_common.z42` 的 `_ensureSeed`
怎么拷种子，得跳到 `build/xtask_compiler.z42` 里去找 `_copyAll`。

## 方案

**NEW `scripts/common/xtask_fs.z42`**，收拢 6 个纯文件系统原语：
`_resetDir` / `_copyAll` / `_linkAll` / `_cleanGlob` / `_copyIfExists` / `_makeExe`。
三条反向依赖同时消失，依赖图变成干净的「各族 → common/」。

规则写进 `scripts/README.md` 新增的「目录分层规则」一节（附「编译器不会拦你、只能靠 review 守」
的警告，和下面的已知余项）。

### 刻意没有收进来的（判据：只有一个文件用，或语义属于某个族而非通用原语）

- `_copyIfNewer`（`build/xtask_compiler.z42`）、`_rmIfExists`（`build/xtask_clean.z42`）
  —— 各自只在同文件用，上移只是把定义推远。
- `_stageCopyExt`（`build/xtask_stdlib.z42`）—— 只在同文件用，**且不能与 `_copyAll` 合并**：
  它经 `_copyIfExists` 直接 `File.Copy`、**不吞异常**（staging CI toolchain artifact，拷贝失败
  必须炸而不是静默产出残包），而 `_copyAll` 是 `try/catch` best-effort。两者形状像、语义相反。
  > 这条是「目录拷贝 6 份实现」那个待办项里**唯一看起来能一比一合并、实际不能**的一对，
  > 记在这里免得下次又推导一遍。
- `_pkgCopyLibs` / `_copyNativeLibs`（`package/xtask_package.z42`）—— 不是原语：带
  `<dir>/libs`、`<dir>/native` 的**布局语义** + 报错文案 + 返回码。`_pkgCopyLibs` 被 `build/`
  的 `_buildSdk` 调用，**仍是一条反向依赖**，但它该去的是将来的「SDK 布局组装」共用层
  （与 `_stageToolchain` 同处），不是 `xtask_fs.z42`。**已知余项，README 里记着。**

## 验证

1. **编译期即证搬全了**：扁平 namespace + 裸名互调 → 少一个函数就是 `E0401 undefined`，
   多一个重名就是重复定义。`z42c build scripts/xtask.z42.toml --release` 0 错误
   （64 文件，比上一版 +1 = 新增的 `xtask_fs.z42`）。
2. `xtask test` 全绿 10/10 stage —— gate 跑遍 `build stdlib`（`_linkAll`/`_cleanGlob`/`_copyAll`）、
   `build compiler`（`_resetDir`）、cross-zpkg（`_copyZpkgs`/`_copyStdlibZpkgs`）这些被搬动的
   原语的调用路径。
   实测 10/10 全绿（build wave 43.5s / stdlib [Test] 1m18s / compiler 23.9s / …）。
3. **`build sdk` 冒烟**：`_buildSdk` 是唯一同时吃 `_resetDir`（原 build/）+ `_makeExe`
   （原 package/）+ `_pkgCopyLibs`（未搬）的调用点，且**不在 GREEN gate 内** → 手动补跑
   `xtask build sdk --out artifacts/.sdk-smoke`：rc=0，产出 `bin/ libs/ programs/ z42`
   四段齐全，`bin/z42vm` 带执行位（`_makeExe` 生效）、`libs/` 25 个 zpkg（`_pkgCopyLibs` 生效）。

## 状态

🟢 完成
