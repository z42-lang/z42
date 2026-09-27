# Design: 命名空间互换 `Z42.Project` ↔ 工程清单

> 状态：**设计中**。User 裁：`Z42.Project`（现装 zpkg 容器格式）→ **`Z42.Package`**；
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
Z42.Build.Project.ManifestLoader.Load / LoadWorkspace   （读清单）
Z42.Project.ZpkgWriterZ.* / ZpkgBuilder.* / ZpkgReader.*（写/读 zpkg）
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
- [ ] **B2**：`seed-run-libs` 快照 + 按代选 `Z42_LIBS`。**不改任何名字**，判据 = 全绿 + `test bootstrap` 过
      （这一步能单独验，正是它要单独做的理由）
- [ ] **B3**：命名空间互换本体（机械替换）。清单**由全仓 grep 得出**，不凭记忆列目录：
      `src/libraries` 52 · `src/compiler` 40 · `src/toolchain` 14 · `scripts` 6 · `docs`(非归档) 24 ·
      `src/runtime` 1（注释）。`.github/` **零命中**（已查）。`docs/spec/archive/**` 不改。
- [ ] **B4**：冷启动验证（`xtask test bootstrap`）**在开 PR 之前**跑 —— #896 的教训

## 5. 已知会踩的坑（来自 #896 三轮 CI）

1. **只扫自己想到的目录必漏**：先 grep、把命中面当清单，再逐类判断（改 / 兼容层 / 归档不动）；
2. **有些代码同时面对改名前后两个版本**：CI 的 base/PR 对比、两代自举、跨 nightly 种子。
   本次 `.github/` 零命中，但 **`test bootstrap` 的 `runlibs` 同时是 `Z42_LIBS` 与 `--output-dir`**，
   必须先分开（B2 覆盖）；
3. **兼容副本不能进编译期 libs**（会判 `E0606` 同 FQN 两个包）；
4. **登记表改 id 格式要同时迁移条目**，否则旧条目成幽灵（#900 修过一次）。
