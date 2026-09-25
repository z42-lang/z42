# Tasks：依赖部署模型

> 设计 SoT：[design.md](design.md)。**批次待 User 裁决 A–D 后定稿**，此处是初步切分。

| 批 | 内容 | bump | 卡 nightly | 状态 |
|----|------|:---:|:---:|------|
| 1 | 统一复制判据 + 统一传递闭包（裁决 B/D）| 否 | 否 | ⬜ |
| 2 | `probing-paths` 旋钮 + 运行期搜索序（裁决 C）| 否 | 否 | ⬜ |
| 3 | `deploy` 字段 support（`DepEntry.Deploy` + ManifestLoader）| 否 | **是** | ⬜ |
| 4 | `deploy` 字段 use（构建期消费 + `shared` 存在性校验）| 否 | 否 | ⬜ |
| X | `Z42_PATH` 死旋钮处置（接通 or 退役）—— **独立立项** | 否 | 否 | ⬜ |

## 批 1 —— 统一判据与闭包（不需要新字段，可立即做）

- [ ] 1.1 抽一个 framework 判据 helper，`_bundleExeDeps` 与 `_pubBundleProjectDeps` 共用：
      判据 = **在不在 shipped `libs/`**。删掉 `_srcRoot` 那条在用户机器上恒退化为
      `StartsWith("z42.")` 的路径判据。
- [ ] 1.2 `_bundleExeDeps` 从「仅直接依赖」改为 BFS 闭包，与 publisher 同规则。
- [ ] 1.3 修正两处互相矛盾的注释（各自声称与对方一致）。
- [ ] 1.4 门：一个「包名叫 `z42.xxx` 的**用户** path 依赖必须被复制」的 e2e ——
      正是今天前缀判据误判的那个形状，**注入实测验判别力**。

## 批 2 —— probing-paths

- [ ] 2.1 新增旋钮 `probing-paths`（`ValueKind::PathList`，toml_key `probing-paths`）。
- [ ] 2.2 `app.rs:110` 的 `search_dirs` 插入展开后的 probing paths（entry 之后、libs 之前）。
- [ ] 2.3 展开器：相对 entry 目录 / 绝对原样 / `*` 与 `**` / Ordinal 稳定排序 / 缺失跳过。
- [ ] 2.4 sidecar：`[runtime] probing-paths` 由 `z42c build` 写出（复用既有 `[runtime]` 段通道）。
- [ ] 2.5 门：**判别力**——把 probing path 接线改 `if (false)` 必须让门变红；另加一格
      「配了不存在的目录不报错」与一格「两个目录同名 zpkg 取声明序第一个」。

## 批 3/4 —— `deploy` 字段

- [ ] 3.1 support：`DepEntry.Deploy`（`""` = 未声明）+ ManifestLoader 解析。**无消费者**
      ⇒ byte-identical、可立即合并。⚠️ 与 role 同形：z42c 读它 = 新跨成员符号，**卡一个 nightly**。
- [ ] 4.1 use：构建期按 `deploy 显式 > framework 默认` 决定复制与否。
- [ ] 4.2 `shared` 的构建期存在性校验（找不到 → 报错，不留到运行期）。
- [ ] 4.3 `role = compile-time` 的包写 `deploy` → 报错（它不在运行期出现）。

## 批 X —— `Z42_PATH` 死旋钮

- [ ] X.1 裁决：接通它原本承诺的 `.zbc` module search 语义，还是明确退役 + 从 `--list-knobs` 移除。
      **不要让它的历史债决定 `probing-paths` 的形状**（见 proposal 裁决 C）。
