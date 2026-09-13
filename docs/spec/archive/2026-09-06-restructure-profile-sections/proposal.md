# Proposal: `[profile.*]` 的两个子表 + 删掉无人读的构建期字段（restructure-profile-sections）

> 状态：🔴 DRAFT | 创建：2026-09-05 | 类型：`lang/工程模型` + `compiler` → 完整流程
> 前置：`add-app-properties`（#469，引入 `[profile.<n>.properties]`）
> User 裁决（2026-09-05）：按此形状重构

---

## Why

`add-app-properties`（#469）给应用属性做了**结构性**分界（`[profile.<n>.properties]` 子表），
但**运行时旋钮没有**——它和构建期键仍靠 `ManifestLoader.z42:268` 那条手写清单分开：

```z42
if (k != "pack" && k != "strip" && k != "optimize" && k != "debug") { /* 当成旋钮 */ }
```

于是 `[profile.*]` 底下有三种东西、只有两种被结构区分。这个不对称是 #469 引入的。

**具体的失败模式**：将来谁加一个构建期键（比如 `lto`）忘了同步那条清单，它就会被烤进
app 的 `runtimeconfig.toml` 随产物发出去，然后每次跑该 app 时 VM 都刷一行
`unknown runtime knob lto`。边界靠一份手工清单守着，是漂移隐患。

**第二个发现**：`[profile.*]` 的 `pack` / `strip` / `debug` / `optimize` / `mode` 被解析进
`Profile` 后**全仓无人读**——`pm.Profiles` 唯一的消费方是侧车生成器
（`RuntimeConfigSidecar.z42` 的三处，只碰 `Knobs` 与 `Properties`）。`pack` 实际在
`[project]` 段下；`mode` 自 `app-config-follows-the-app` 后走 `Knobs`。

而 4 个工具链 manifest 里现在都写着 `optimize = 2`——作者大概以为它生效了。

---

## What

### A. 三分变两个子表，规则从「排除清单」变成「结构」

```toml
[profile.release]                # 具名配置，本身只是命名空间
[profile.release.runtime]        # → 侧车 [runtime]：VM 旋钮
mode    = "interp"
gc-mode = "concurrent"
[profile.release.properties]     # → 侧车 [properties]：应用属性
api-endpoint = "https://prod"
```

两个子表与侧车两段 **1:1**。deny-list 直接删掉——加任何一边都不会泄到另一边。

### B. 删掉无人读的 `Pack` / `Strip` / `Mode` / `Optimize` / `Debug`

它们进 `Profile` 后没有任何消费方。留着只会让人以为有用（`optimize = 2` 就是现成的
例子）。删字段 + 删解析 + 清掉仓内 manifest 里那几处。

> **名字不改**：`profile` 保持 Cargo 的含义（具名的构建配置）。需要名字的是它里面那一层，
> 现在有了（`.runtime` / `.properties`）。

### C. 迁移仓内 6 个 manifest

4 个工具链（launcher / devtools / interactive / testagent）+ 2 个 multi-exe fixture，
都只是把 `mode = "interp"` 挪进 `[profile.X.runtime]`、删掉 `optimize = 2`。

---

## What This Does NOT Do

- **不做未知键的编译期检查**：`[profile.X.runtime]` 里写错旋钮名该 warning——但那需要
  编译器能看到旋钮登记表，是**下一个 change**（旋钮登记表变成编译器与 VM 共享的配置）。
  本 change 只重构形状。
- **不改优先级链、不改侧车格式**（侧车两段本来就叫 `[runtime]` / `[properties]`）。
- **不做兼容**：pre-1.0，一次切干净。旧形状（`[profile.X] mode = ...`）解析后**不再**
  成为旋钮——它会变成 `[profile.X]` 下的未知标量，见 D。

### D. 旧形状的过渡手感

删掉 `Pack/Strip/Mode/Optimize/Debug` 后，`[profile.X]` 自己的标量**一个都不认**。
写在那里的键会被静默忽略——这对正在迁移的人很坏（`mode = "interp"` 突然不生效且无提示）。
故：**`[profile.X]` 下出现任何标量 → 一条明确的构建期错误**，指出旋钮该写进
`[profile.X.runtime]`。一次切干净 + 明确报错，好过静默失效。

---

## 三阶段

| 阶段 | 内容 | 风险 |
|---|---|---|
| **P0** | `Profile` 模型删 5 个死字段、加 `.runtime` 解析；`[profile.X]` 下的标量报错 | 中（碰 stdlib 模型）|
| **P1** | 侧车生成器跟随（`Knobs` 来源变了，逻辑不变）| 中（碰自举）|
| **P2** | 迁移仓内 6 个 manifest + 文档 | 低 |

## Scope
`z42.project`（`Profile` / `ManifestLoader`）· `z42c.driver`（`RuntimeConfigSidecar.z42`）·
6 个 manifest · `docs/book/src/runtime/runtime-settings.md` + `stdlib/app-properties.md`

## 未决
无。
