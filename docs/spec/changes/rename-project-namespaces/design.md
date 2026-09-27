# Design: 命名空间互换 —— zpkg 容器格式 ↔ 工程清单

> 状态：**B3 已实现**（本地 `build stdlib` 全绿）。User 裁：
> `Z42.Project`（现装 zpkg 容器格式）→ **`Z42.Package`**；
> `Z42.Build.Project`（工程清单模型）→ **`Z42.Project`**。
> 前置 `--compile-libs` = #894 + #900。

## 1. 为什么要换

名字装错了内容：

| 包 | 命名空间 | 里面是什么 |
|---|---|---|
| `z42.package` | `Z42.IR` + **`Z42.Project`** | 后者是 **zpkg/zbc 容器读写**（`ZpkgReader` / `ZpkgWriterZ` / `ZpkgBuilder` / `SidecarReader`）|
| `z42.project` | **`Z42.Build.Project`** | **工程清单模型**（`ProjectManifest` / `ManifestLoader` / `DepEntry` / `Profile`）|

即 `Z42.Project` 指的是「打包格式」，而真正的工程清单只能退到 `Z42.Build.Project`。
这是 `converge-z42c-ir-metadata-onto-stdlib` 把 `z42c.project`（写 zpkg 的那个）并入时留下的。

## 2. 为什么它比包名改名难一个数量级

包名**不出现在任何 FQN 里**，所以 `z42.ir` → `z42.package`（#896）只要一份旧文件名的运行期
兼容副本。命名空间改名会**抹掉旧 FQN**，而上一代 z42c 二进制在运行期按旧 FQN 调用：

```
Z42.Project.ManifestLoader.Load / LoadWorkspace   （读清单）
Z42.Package.ZpkgWriterZ.* / ZpkgBuilder.* / ZpkgReader.*（写/读 zpkg）
```

⇒ 文件副本救不了；z42 的 `using X = T;` 是**文件级类型替换**、不导出新 FQN，也当不了桥。

## 3. 🔴 关键约束：`Z42_LIBS` 必须与 driver 的「代」匹配

`--compile-libs`（#894/#900）把**编译期**面分出来了，但**运行期**那一档仍需按代切换：

| 阶段 | 跑哪代 driver | 运行期要 | 编译期要 |
|---|---|---|---|
| `build compiler` 的 workspace 自建 | **种子**（旧 FQN） | **旧** libs | 新（`--compile-libs`） |
| 之后 `build stdlib` 步骤 4 / 一切用 gen1 的步骤 | **gen1**（新 FQN） | **新** libs | 新 |

⇒ 不是「固定一个 `Z42_LIBS` + 一个 `--compile-libs`」。需要：
1. `_ensureSeed` 之后立刻把 flat 快照成 **`seed-run-libs/`**（种子代，全程不被写）；
2. **用种子 driver 的调用** → `Z42_LIBS=seed-run-libs`、`--compile-libs=<fresh>`；
3. **driver 换代之后的调用** → 两者都用 fresh。

这与仓库为格式 bump 准备的「多一代收敛」是同一套思路（`ci-bootstrap` 的 gen0/gen1/gen2）。

## 4. 分步（每步能单独验）

- [x] **前置 A**：`--compile-libs`（#894）
- [x] **前置 B**：workspace 路径也认它（#900）
- [x] **B2**：按代选 `Z42_LIBS`（#902）
- [x] **B3**：命名空间互换本体。清单**由全仓 grep 得出**，不凭记忆列目录：
      `src/libraries` 52 · `src/compiler` 40 · `src/toolchain` 14 · `docs`(非归档) 24 ·
      `src/runtime` 1（注释）。`.github/` **零命中**（已查）。`docs/spec/archive/**` 不改。
      🔴 **`scripts/` 不在本轮**——理由见 §6
- [ ] **B4**：冷启动验证（`xtask test bootstrap`）**在开 PR 之前**跑 —— #896 的教训
- [ ] **B5**（下一 nightly 后）：扫 `scripts/` 的 4 处 `using Z42.Build.Project`
      —— 已挂 `STAGE2-DEBT(b3-scripts-ns-sweep)`
- [ ] **B6**（z42c 侧，非阻塞）：修解析分档 —— **成员 dist 不该回答外部包**，见 §7

## 5. 已知会踩的坑（来自 #896 三轮 CI）

1. **只扫自己想到的目录必漏**：先 grep、把命中面当清单，再逐类判断（改 / 兼容层 / 归档不动）；
2. **有些代码同时面对改名前后两个版本**：CI 的 base/PR 对比、两代自举、跨 nightly 种子。
   本次 `.github/` 零命中，但 **`test bootstrap` 的 `runlibs` 同时是 `Z42_LIBS` 与 `--output-dir`**，
   必须先分开（B2 覆盖）；
3. **兼容副本不能进编译期 libs**（会判 `E0606` 同 FQN 两个包）；
4. **登记表改 id 格式要同时迁移条目**，否则旧条目成幽灵（#900 修过一次）。

## 6. 🔴 `scripts/`（xtask）是跨代消费者，本轮**不能**跟着改

xtask 不像 `src/toolchain`（从源码编、对着 flat 跑）。它两头都挂在**上一代 SDK** 上：

| 环节 | libs 来源 | 哪一代 |
|---|---|---|
| CI `ci-bootstrap` 步骤 2「seed z42c builds current xtask.zpkg」| `Z42_LIBS=$boot_libsw`（下载的 nightly SDK libs）| **上一代** |
| CI 步骤 3+ 运行 xtask | 同上 | **上一代** |
| 本地 `./xtask` | apphost 从 `Z42_HOME/libs`（= 安装的 SDK）加载 | **上一代** |

⇒ 本轮就把 `scripts/` 改成新名，种子连 xtask.zpkg 都编不出来（`E0494`），自举第一步即断。
这正是 `bootstrap-seed.md` 的「support 先行、晚一个 nightly 再 use」：**B3 提供新名，B5 才消费**。
本地改完 xtask 源码后重建要钉种子代 libs：`Z42_LIBS=.z42/libs .z42/bin/z42c build scripts/xtask.z42.toml --release`。

## 7. 🔴 B3 实测挖出的两个真缺陷（都不是改名本身）

### ① 成员 dist 在编译期**遮蔽** flat（`E0494`，根因在 z42c）

`WorkspaceBuild.z42` 的解析面是 `libsDirs = 全成员 dist + 外部档（--compile-libs / Z42_LIBS）`，
**纯 basename 命中、成员 dist 排在前**。而成员 dist 同时是**运行期载荷**目录——`z42c build` 对 exe
会 colocate 依赖闭包进去。于是上一代 colocate 的外部包副本盖住 flat 里刚建好的当前源新版：

```
z42c.driver/release/dist/z42.package.zpkg（种子代，声明 Z42.Project）
  遮蔽 artifacts/build/libraries/dist/release/z42.package.zpkg（当前源，声明 Z42.Package）
⇒ z42c.semantics: E0494 命名空间 `Z42.Package` 不存在   ——而 flat 里明明有
```

⚠️ **两份同尺寸**（`Z42.Project` / `Z42.Package` 都 12 字符），只核对文件大小会被骗过去；
判别要看 zsym 的 STRS 段，且命名空间按**段**存（`\x03Z42\x07Package`），整串在二进制里 grep 不到。

既有的 `_laterMemberNames`（藏后序成员）是**同一类**问题的局部补丁：它只按名挡成员，不管
成员 dist 里的**非成员**副本。正解（B6）= 成员 dist 只对**成员名**有效，外部包一律走外部档。
但跑 workspace 构建的是**种子** driver ⇒ 那个修法自己也受种子纪律约束，要跨一个 nightly 才生效
（与 #902 学到的「enabler 自己也是种子纪律的对象」同一课），故编排侧先解（见 ②）。

### ② flat 当不了「代」的锚（`MissingSymbolException`）

B2 的做法是「把 flat 快照成 `seed-run-libs` 当种子代运行期面」。但 flat 正是本轮要被
`_ensureBootstrapSelfDepLibs` 覆盖成**当前源**的目录 —— 快照晚于覆盖、或上一轮跑到一半，
快照拿到的就是**新代**，种子 driver 随即
`MissingSymbolException: Z42.Build.Project.ManifestLoader.LoadWorkspace`。

改法：**锚换成 driver 自己 dist 里那份 colocated 闭包**（与 driver 同代是*构造保证*的），
把它**搬**进 `seed-run-libs` —— 一举同时解掉 ①（成员 dist 不再有外部包）和 ②（拿到真·种子代）。
判据不是「搬到了吗」而是「**齐了吗**」：bundle 可能不完整（冷启动 staged 的 driver 根本没 bundle；
被清理过的树只剩零星几个），缺口从 **SDK libs** 补（`_ensureSeed` staged 种子正是从那儿取的，同代）。
实现见 `scripts/build/xtask_compiler.z42` 的 `_relocateSeedRunLibs` / `_topUpSeedRunLibs`。

> 教训：判定「哪一代」不能看**会被写的目录**。锚要选与产物同生共死的东西——这里是
> driver 自己的 bundle，退而求其次是 SDK（种子的出处），**永远不是 flat**。
