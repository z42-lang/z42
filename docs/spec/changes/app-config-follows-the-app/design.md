# Design: app-config 层跟着 app 走

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)

## Decisions

### Decision 1：约定归 VM 一处；调用方只在"路径已在手里"时转发

侧车的发现约定（**同目录、同 stem**）此前散在三处：launcher、apphost（#446）、
以及"谁都没做"的那些入口。三处编码同一个约定 = 三处可以各自漂移。

**规则**：约定的**唯一实现**在 `config::sidecar_for`。调用方**可以**传显式路径
（`Z42_APP_CONFIG`），但**不需要**自己去发现。

据此：

- **apphost 的发现删掉**——它算出路径只为交回给能自己算的那个东西，是纯重复。
  apphost 的本分是"找 z42vm + 把 app 交给它"（`simplify-apphost-direct-run`）。
- **launcher 保留**——它为了顶层 `version` pin 本来就把侧车**读进来**了
  （`_appRuntimeConfig` + `TomlValue.Parse`），路径已在手里。传下去是转发已知值，
  不是重复发现。而且它跨版本 pin 运行时，显式传递对版本偏斜更稳。

### Decision 2：推导发生在**解析之前**，不是"运行时补一层"

配置在 `OnceLock` 里 boot 后冻结，所以 app 文件路径必须在**装配层**的时候就已知。

- `z42vm main()`：`cli.file` 在 clap 解析后立刻可得，早于 `init_runtime_config`。
- `z42-host::run_app(file, ..)`：`file` 是它的第一个参数；在调 `z42::app::run` 前装配。

不把推导塞进 `z42::app::run`（两者共用的核心）：那时 `main()` 已经装配并冻结了配置，
`app::run` 再想加一层就晚了。**两个入口各调一次同一个 helper** 比"核心里做一半、
入口再做一半"清楚。

### Decision 3：`z42vm --show-config <app>` 也应看到 app 层

`--info` / `--list-knobs` / `--show-config` 此前不要求 `<FILE>`。给了文件时，推导照做
——于是 `z42vm --show-config dist/demo.zpkg` 直接回答"这个 app 跑起来会用什么设置"，
这是排查配置最自然的问法。不给文件就没有 app 层，与现状一致。

### Decision 4：`with_extension` 的替换语义正是要的

`app.zpkg` → `app.runtimeconfig.toml`（替换最后一个扩展名，不是追加）。
`demo.zpkg` → `demo.runtimeconfig.toml`。与 `z42c build` 的产出、launcher 的
`_runtimeConfigPath`、以及 `z42 publish` 的 `_pubSidecarOf` 三者一致——
本 change 之后前两者是**唯一**还编码这条约定的地方（publish 侧是"拷贝时改名"，
必须自己算目标名）。

### Decision 5：不存在则安静跳过

多数工程没有 `[profile.*]` 运行时旋钮 ⇒ `z42c build` 不产侧车 ⇒ 推导出的路径不存在。
这是**常态**，不是异常：不 warn、不报错。这与 `Z42_APP_CONFIG` **被显式设成**一个
不存在的路径不同——那种情况仍然 warn（用户明确说了要用它）。

## Implementation Notes

- `sidecar_for` 放 `config/source.rs`（配置文件层的家），签名
  `pub fn sidecar_for(app_file: &Path) -> Option<PathBuf>`：只做路径推导 + `is_file()`，
  不读文件（读由既有的 `load_config_file` 负责）。
- `main.rs`：`Inputs.app_config` 现在来自
  `load_layer(get, "Z42_APP_CONFIG")` **或** `sidecar_for(cli.file)`。显式优先。
- `z42-host::run_app`：在 `z42::app::run` 之前 `init_runtime_config`；若已被装配
  （宿主自己先装过、或已有代码读过 `runtime_config()`）→ 保持现状不覆盖。
- 删 `hostrun.rs` 的 `app_config_sidecar` + 它的 4 个单测；apphost 的 `exec_app` 回到
  只设 `Z42_LIBS`。

## Testing Strategy

| 层 | 测试 |
|---|---|
| `sidecar_for` | `app.zpkg` → `app.runtimeconfig.toml`；不存在 → None；是目录 → None；`.zbc` 也行 |
| 显式优先 | `Z42_APP_CONFIG` 已设 → 推导不参与 |
| 解析链 | 推导出的层落在 `app-config`，被 `Z42_CONFIG` 逐 key 压过 |
| 不存在安静 | 无侧车时无 warn；显式指向不存在的路径时仍 warn |
| e2e | `z42vm <payload>.zpkg` 直跑 → `source=app-config`（**当前是 `default`**）；`--show-config <app>` 显示该层 |
| 非破坏 | 不给文件时行为不变；launcher / apphost 路径仍对（dist smoke）|
