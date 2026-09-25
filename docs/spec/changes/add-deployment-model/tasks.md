# Tasks：依赖部署模型

> 设计 SoT：[design.md](design.md)。**批次待 User 裁决 A–D 后定稿**，此处是初步切分。

| 批 | 内容 | bump | 卡 nightly | 状态 |
|----|------|:---:|:---:|------|
| 1 | 统一复制判据 + 统一传递闭包（裁决 B/D）| 否 | 否 | ⬜ |
| 2 | `probing-paths` 旋钮 + 运行期搜索序（裁决 C）| 否 | 否 | ⬜ |
| 3 | `deploy` 字段 support（`DepEntry.Deploy` + ManifestLoader）| 否 | **是** | ⬜ |
| 4 | `deploy` 字段 use（构建期消费 + `shared` 存在性校验）| 否 | 否 | ⬜ |
| X | `Z42_PATH` 死旋钮处置（接通 or 退役）—— **独立立项** | 否 | 否 | ⬜ |

## 批 1 —— 统一复制判据 ✅

- [x] 1.1 `_bundleExeDeps` 的判据换成 **在不在 shipped `libs/`**（= `Z42_LIBS`，publisher 一直
      在用的那条），删掉 `_srcRoot`（唯一调用方就是它）。**没抽共用 helper**：两个函数分别住在
      `z42c.driver` 与 `z42.builder`，共用 helper 要落在两边都依赖的包里 = 新跨成员符号、卡一个
      nightly；判据本身只有一行，两边各写一遍 + 注释互指更稳（同仓库既有做法）。
- [x] 1.2 顺带删掉不再使用的 `projectDir` 参数（三个调用点同步）。
- [x] 1.3 修正互相矛盾的注释，并把「只复制直接依赖」这条缺口**写在代码里**（连同为什么现在
      修不了：z42 侧读不出 DEPS 段）。
- [x] 1.4 门 `_e2eDeployPredicateChecks`（`xtask_compiler_e2e_deploy.z42`）。
      🔴 **fixture 必须落 repo 外（`/tmp`）**：放 repo 内 `_srcRoot` 找得到根、走目录判据，
      旧代码在那儿是对的 —— 门就看不见差别。**这个缺陷只在用户机器上发生。**
      判别力实证：把判据退回 `dep.StartsWith("z42.")` → 门红在「仍被复制进 exe dist」、rc=1。

> **可观测差异是什么**（选门的场景时想清楚的）：新旧判据对 path 依赖结论相同（旧代码里
> `!isPathDep &&` 已经把 path 依赖排除在 stdlib 之外）。真正的差异是**一个确实在框架目录里、
> 但名字不以 `z42.` 开头的包** —— 旧判据把它当私有依赖复制进每个用到它的 exe。

## 批 2 —— zpkg DEPS 解码 + 统一闭包

- [ ] 2.1 **support**：z42 侧 `ZpkgReader` 补 DEPS 段解码（Rust 侧 `zbc_reader/zpkg.rs:46`
      已有，z42 侧没有）。无消费者 ⇒ byte-identical、可立即合并；卡一个 nightly 才能被 use。
- [ ] 2.2 **use**：`_bundleExeDeps` 改 BFS 闭包，依赖来源 = zpkg 的 DEPS 段（**不是**源码树
      toml —— 那条路在 repo 外整个失效）。
- [ ] 2.3 🔴 **publisher 的闭包在 repo 外失效**（`srcRoot == ""` ⇒ 每个依赖 continue，零复制
      零递归）也一并修：改按 DEPS 段或 `{ path }` 解析，镜像 `_pubBundleProjectNativeDeps`
      的 Decision 5（那条已经修过，zpkg 这条漏了）。

## 批 3 —— probing-paths

- [ ] 3.1 新增旋钮 `probing-paths`（`ValueKind::PathList`，toml_key `probing-paths`）。
- [ ] 3.2 `app.rs:110` 的 `search_dirs` 插入展开后的 probing paths（entry 之后、libs 之前）。
- [ ] 3.3 展开器：相对 entry 目录 / 绝对原样 / `*` 与 `**` / Ordinal 稳定排序 / 缺失跳过。
- [ ] 3.4 sidecar：`[runtime] probing-paths` 由 `z42c build` 写出（复用既有 `[runtime]` 段通道）。
- [ ] 3.5 门：**判别力**——把 probing path 接线改 `if (false)` 必须让门变红；另加一格
      「配了不存在的目录不报错」与一格「两个目录同名 zpkg 取声明序第一个」。

## 批 4 —— `deploy` 字段

- [ ] 4.0 support：`DepEntry.Deploy`（`""` = 未声明）+ ManifestLoader 解析。**无消费者**
      ⇒ byte-identical、可立即合并。⚠️ 与 role 同形：z42c 读它 = 新跨成员符号，**卡一个 nightly**。
- [ ] 4.1 use：构建期按 `deploy 显式 > framework 默认` 决定复制与否。
- [ ] 4.2 `shared` 的构建期存在性校验（找不到 → 报错，不留到运行期）。
- [ ] 4.3 `role = compile-time` 的包写 `deploy` → 报错（它不在运行期出现）。

## 批 X —— `Z42_PATH` 死旋钮

- [ ] X.1 裁决：接通它原本承诺的 `.zbc` module search 语义，还是明确退役 + 从 `--list-knobs` 移除。
      **不要让它的历史债决定 `probing-paths` 的形状**（见 proposal 裁决 C）。
