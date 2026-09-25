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

## 批 2 —— zpkg DEPS 解码 + 按名依赖的闭包

> ⚠️ **订正**：初稿把「统一闭包」整块判成做不到。实际上 **path 依赖那半已由 #811 修掉**
> （闭包本来就有，`PathDepPlan.Resolve`，只是没透传给 `_bundleExeDeps`）。剩下的是按名那半。

- [x] 2.1 **support**：`ZpkgReader.ReadDependencies`（Rust 侧 `zbc_reader/zpkg.rs:46` 早有，
      z42 侧没有）。无消费者 ⇒ 自举 byte-identical、已合并；use 待它随 nightly 进种子。
      门 = `zpkg.z42` 两条往返测试。⭐ **第一版 fixture 没有判别力**：把「按 nsCount 跳过」
      改成「恒跳 1 个」注入进去，测试**照样绿** —— 因为多命名空间的那一项被我放在了**最后**，
      跳错多少都没有后续项会错位。多 ns 项前移后才真红。
      **「构造了一个复杂输入」不等于那个复杂度真的被验到了。**
- [ ] 2.2 **use**：`_bundleExeDeps` 对**按名引用**的依赖也建闭包，来源 = zpkg 的 DEPS 段
      （**不是**源码树 toml —— 那条路在 repo 外整个失效）。
- [ ] 2.3 🔴 **publisher 的闭包在 repo 外失效**（`srcRoot == ""` ⇒ 每个依赖 continue，零复制
      零递归）也一并修：改按 DEPS 段或 `{ path }` 解析，镜像 `_pubBundleProjectNativeDeps`
      的 Decision 5（那条已经修过，zpkg 这条漏了）。

## 批 3 —— probing-paths

- [x] 3.1 新增旋钮 `Z42_PROBING_PATHS` / `probing-paths`（`ValueKind::PathList`）。
      ⭐ **z42 侧零改动**：profile knobs 是通用 `"key=value"` 透传，且旋钮名**直接问 VM**
      （`RuntimeConfig.Names()`）⇒ 新旋钮进了 VM 登记表，z42c 自动认识、侧车自动烤进去。
- [x] 3.2 `app.rs` 的 `search_dirs` 插入展开结果（entry 之后、libs 之前）。
- [x] 3.3 展开器（`runtime/src/probing.rs`）：相对 **entry 目录**／绝对原样／`*` 与 `**`／
      只返回目录／Ordinal 稳定排序／缺失静默跳过。
      ⭐ **单测抓到一个会漏掉的缺陷**：`entry.join("../shared")` 的字面量是 `app/../shared`，
      与 `shared` 是**不同字符串** ⇒ 去重失效、同一目录搜两遍。加**词法**规范化（不用
      `canonicalize` —— 那会解析符号链接，改变用户写的语义）。
- [x] 3.4 侧车：`[profile.<n>.runtime] probing-paths` → `dist/<app>.runtimeconfig.toml`。
- [x] 3.5 两层门：`probing_tests.rs`（展开规则 8 格）+ e2e `_e2eProbingPathChecks`（三格：
      不配→跑不起来／配了→跑得起来／改旋钮值→侧车跟着变）。
      判别力实证：把 `search_dirs` 的接线换成空列表 → 门红在「配了却没生效」、rc=1。

### 批 3 顺带修掉的真缺陷

🔴 **所有运行时旋钮「改了但不生效」**：`[profile.*.runtime]` 与 `[properties]` **不进源 hash**
（它们不影响编译产物），于是「只改运行时配置、源码一字未动」恰好全命中增量缓存 ⇒ 走
preserved 早退 ⇒ **侧车留在上一次的值**。实测：probing-paths 从 `../../shared` 改成
`../../CHANGED`，重建报成功而侧车纹丝不动。修 = preserved 分支里也写侧车（幂等）。
**射程不止 probing-paths，是每一个运行时旋钮。**

### 已知限制

`probing-paths` 是**平台分隔符**分隔的字符串（与 `path`／`native-path` 一致），跨平台清单写多条
时分隔符不同。数组写法要改清单模型（`pr.Knobs` 是扁平 `"key=value"`）= 卡 nightly ⇒ 随批 4 的
清单改动一起做。

另：`deploy = "shared"` 的依赖**编译期仍须可解析**（z42c 要读它的元数据），probing-paths 只管
运行期 —— fixture 要按「构建机有完整 libs、目标机只有 shared/」来搭。

## 批 4 —— `deploy` 字段

- [x] 4.0 support：`DepEntry.Deploy`（`""` = 未声明）+ ManifestLoader 解析。**无消费者**
      ⇒ byte-identical、已合并。**构造后赋值、不进 ctor 签名** —— ctor 是种子 ABI 的一部分，
      加参数会让上一版 z42c 编不动当前源码（同 `Pipeline.ParentPkg` 那几个「构造后填」字段）。
      解析层只忠实搬运、**不校验取值**：合法取值是消费方的事。
- [ ] 4.1 use：构建期按 `deploy 显式 > framework 默认` 决定复制与否。
- [ ] 4.2 `shared` 的构建期存在性校验（找不到 → 报错，不留到运行期）。
- [ ] 4.3 `role = compile-time` 的包写 `deploy` → 报错（它不在运行期出现）。

## 批 X —— `Z42_PATH` 死旋钮

- [ ] X.1 裁决：接通它原本承诺的 `.zbc` module search 语义，还是明确退役 + 从 `--list-knobs` 移除。
      **不要让它的历史债决定 `probing-paths` 的形状**（见 proposal 裁决 C）。
