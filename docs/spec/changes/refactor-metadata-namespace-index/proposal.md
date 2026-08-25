# Proposal: metadata namespace 解析与加载生命周期分层（NamespaceIndex）

> runtime_review #6 第二步。第一步 `refactor-metadata-loader-split`（纯搬移拆
> `loader.rs` 1237→5 子模块）已合并 main（PR #289）。本步做**行为层面的职责归一**。

## Why

`loader` 与 `lazy_loader` 两模块各自实现「从 zpkg 解析命名空间 → 定位候选 zpkg」，
并且**已经耦合**：

- `lazy_loader.rs:564` 的建候选循环**反向调用** `loader::resolve_namespace`（磁盘扫描），
  同时又自己用 `ZpkgCandidate::build`（`read_zpkg_meta`）读同一批 zpkg 的 namespace。
- `loader::find_namespace_in_zpkg_dirs` 与 lazy_loader 建 `declared_zpkgs` 的逻辑都是
  「遍历 libs 目录 → 逐 zpkg 读 NSPC 段 → 建 namespace↔path 映射」，只是**结果的持有方式**
  和**匹配策略**不同。

问题不是「两个查询函数长得像」（它们的差异是真实的，见 Out of Scope），而是**「扫目录+抽
namespace」这段无状态解析逻辑被复制了两份，且跨模块反向依赖**。

复盘发现代码本就是三层，但中间层被切碎在两个模块里：

| 层 | 现状 | 本 change |
|---|---|---|
| 字节解析 `zbc_reader::read_zpkg_meta` / `read_zpkg_namespaces` | 已共享 | 不动 |
| **扫目录 → namespace↔path 索引** | **复制两份 + 跨模块反向依赖** | **提取 `NamespaceIndex` 无状态原语** |
| 加载/追踪/释放（`loaded_zpkgs`、按需 load） | lazy_loader 独有 | 不动，留 lazy_loader |

## 核心洞见：解析 vs 生命周期（owned 结果，caller 决定弃/留）

分界线在**谁持有解析结果**：

- **loader**（transient 消费者）：扫目录读完 namespace 就 `drop` 字节、返回 path。用完即弃。
- **lazy_loader**（retaining 消费者）：把候选（path + namespaces）**长期存进 VmContext**，
  配 `loaded_zpkgs` 做按需加载生命周期。

→ 提取的 `NamespaceIndex` 是**无状态原语**，返回 **owned 数据**，由调用方自己决定
「用完 drop」（loader）还是「留存做生命周期」（lazy_loader）。**生命周期本身不进原语**。

## What Changes

### 新增 `metadata/namespace_index.rs`（无状态原语）

```rust
/// 一条已解析的 zpkg 候选：路径 + 它导出的命名空间列表。owned，可被 caller drop 或 retain。
pub struct NsCandidate { pub path: PathBuf, pub namespaces: Vec<String> }

/// 扫描 libs 目录集，对每个 .zpkg 读 NSPC 段，产出 owned 候选列表。
/// common-pitfalls §1：目录内按文件名稳定 sort 后再产出（确定序）。
/// 无状态：不持有任何东西，返回即交给 caller。
pub fn scan_zpkg_candidates(dirs: &[PathBuf]) -> Vec<NsCandidate>;

/// 同理扫 .zbc 目录（loader 的 zbc-tier override 用）。
pub fn scan_zbc_candidates(dirs: &[PathBuf]) -> Vec<NsCandidate>;
```

（`scan_*` 内部复用现有 `zbc_reader::read_zpkg_namespaces` / `read_zbc_namespace`——
字节解析层不重写。）

### loader（transient 消费者）

`resolve_namespace` 改为：`scan_zbc_candidates` + `scan_zpkg_candidates` → **精确匹配过滤**
→ 返回 path → 候选列表随函数返回被 drop。行为字节不变（精确匹配 + zbc-override 语义保留）。
删除 `find_namespace_in_zbc_dirs` / `find_namespace_in_zpkg_dirs`（逻辑移入 `scan_*`）。

### lazy_loader（retaining 消费者）

- `declared_zpkgs` 的建候选循环改为消费 `scan_zpkg_candidates` 的 owned 结果（**保留**进
  `declared_zpkgs`），不再反向调 `loader::resolve_namespace` → **消除跨模块反向依赖**。
- `candidates_for_namespace` 保持**前缀匹配 + `loaded_zpkgs` 过滤 + sort**（生命周期查询，
  不动）。
- 按需加载 / `loaded_zpkgs` 追踪 / 释放语义**完全不动**。

## Scope

- `metadata/namespace_index.rs`（新）
- `metadata/loader/namespace.rs`（`resolve_namespace` 改写、删两个 `find_*`）
- `metadata/lazy_loader.rs`（建候选循环改消费原语、去 `loader::resolve_namespace` 调用）

## Out of Scope（刻意不并，避免强凑）

- **不合并** `resolve_namespace`（精确匹配、返回 PathBuf）与 `candidates_for_namespace`
  （前缀匹配、返回 String 文件名、过滤 loaded）——两者匹配策略/返回型/状态本质不同，强并
  会引入条件参数把两条语义揉一起。它们各自消费同一个 `NamespaceIndex` 原语即可。
- **不动** `seed_types_for_lookup` vs `build_type_registry`——前者播种 lookup map、后者是
  重量级类型注册 pass（topo + vtable + field slots），目的不同，非本 change 的「namespace
  索引」范畴。review M1 把它们并列列出，但它们不属于同一个抽象。
- **不动** resolver（token 解析 + inline cache）——review 已明确其职责独立。
- **不改** zbc/zpkg 格式，零格式 bump。

## 验证计划

- `cargo test --lib`：现有 `loader::tests`（resolve_namespace 精确/override/歧义）+
  `lazy_loader::tests`（候选前缀匹配/first-dir-wins/去重）覆盖两个消费者的匹配语义；
  新增 `namespace_index` 单测验 `scan_*` 的确定序 + NSPC 解析。
- **端到端命名空间路由**（跨 zpkg lazy load、`SortedSet.Add` 类 VCall 路由）`cargo test --lib`
  覆盖不到，需 `xtask test` 全 stage（含自举 + cross-zpkg）——**本机 z42vm 退出期挂起跑不了，
  以 PR CI 为准**（build-and-test 各 OS + self-host + cross-zpkg gate）。
- 因碰运行期加载语义：合并前必确认 CI 的 cross-zpkg / stdlib-jit / self-host job 全绿。

## 风险

命名空间解析正确性是 lazy-load 路由的地基，回归会表现为「跨包调用 VCall/Call not found」。
`cargo test --lib` 的单测能挡住匹配策略与确定序回归，但真实多-zpkg 路由链只有 CI 端到端能验。
故本 change 明确 **CI-gated**，不以本机 warm 跑通为「全绿」判据。
