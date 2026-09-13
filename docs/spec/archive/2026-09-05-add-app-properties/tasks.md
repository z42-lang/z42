# Tasks: add-app-properties

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/app-properties/spec.md)

## P0 — manifest 模型与解析（stdlib / z42.project）
- [x] 0.1 `ProjectManifest` 加 `Properties: TomlValue`（顶层 `[properties]`，缺省空表）
- [x] 0.2 `Profile` 加 `Properties: TomlValue`（`[profile.<n>.properties]`，缺省空表）
- [x] 0.3 `ManifestLoader`：解析两处；`_profileKnobs` 确认仍只收标量（子表天然被跳过）
- [x] 0.4 GREEN

## P1 — 侧车生成（compiler / z42c.driver）⚠ 碰自举
- [x] 1.1 合并：基表浅拷贝 + profile 顶层 key 覆盖
- [x] 1.2 改用 `TomlValue.Stringify` 构造整份侧车（`[runtime]` + `[properties]`），**删掉手拼**
- [x] 1.3 两段都为空 → 仍不产文件
- [x] 1.4 自举不动点 gen1==gen2

## P2 — VM 读取 + 脚本表面
- [x] 2.1 `config/source.rs`：从 app-config 文件读 `[properties]`（与 `[runtime]` 并列）
- [x] 2.2 `RuntimeConfig` 加 `app_properties: Option<toml::Table>`（**不进 resolved / KNOWN_KNOBS**）
- [x] 2.3 用户配置里出现 `[properties]` → warn 一行
- [x] 2.4 `corelib/appprops.rs`：`__app_prop` / `__app_prop_has` / `__app_prop_names` /
      `__app_props_toml`（追加注册，保 BuiltinId）
- [x] 2.5 `z42.core/src/Runtime/AppProperties.z42`：`Get` / `Has` / `Names` / `Raw`
- [x] 2.6 单测 + z42 e2e（标量 + 嵌套表 round-trip）
- [x] 2.7 GREEN

## 文档
- [x] `docs/book/src/stdlib/app-properties.md`（NEW）+ SUMMARY
- [x] `docs/book/src/runtime/runtime-settings.md`：加「旋钮 vs 属性」的分界说明

## 未决
无。


## 落地记录（2026-09-05）

在 worktree `wt-appprops`（基于 origin/main）里做。

**逐项核对过，不是正则批量翻**——上一轮就是那么翻出的一条假勾（见
`complete-runtime-settings/tasks.md` 的注记）。

**手工端到端**（一次覆盖全部形状）：manifest 写基表 + `[profile.debug.properties]`
覆盖 + 数组 + 嵌套表 → `z42c build` → 侧车形状正确（profile 覆盖了基表的
`api-endpoint`）→ **`z42vm dist/app.zpkg` 直跑**（无 launcher、无环境变量）→
`Main()` 里读到：

```
name=demo               endpoint=http://localhost:8080   ← profile 覆盖基表
has(limits)=true        get(limits)=(non-scalar → null)  ← 标量口子如实拒绝结构化值
names=4                 retries=3                        ← 嵌套表（Raw + Std.Toml）
flag0=x                 knob mode src=app-config         ← 旋钮走自己的通道
```

**没有** "unknown runtime knob" 警告——分表生效。

**中途一次自己的失误**：测试代码里用 `TomlValue.Get(0)` 访问数组元素，正确的是
`At(i)`。功能没问题，是我写测试时的 API 记错。

**GREEN**：runtime cargo 1163/0（新增 4 个分表单测）；stdlib 25/25；
其余门禁见下方 commit。

## 剩余
- `[profile.*]` 里旋钮与构建期键混住、靠 deny-list 分界（User 已明确暂不处理）
- `[profile.*].pack / strip / debug / optimize` 解析后无人读（同上，已记录）
