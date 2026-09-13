# Tasks: fix-projecthooks-vtable-fixup

> 状态：🟢 已完成 | 类型：fix（runtime，最小化模式）

**变更说明：** `try_fixup_inheritance`（跨 zpkg 继承 fixup 的阶段 2）把 `Arc::get_mut` 改为
`Arc::make_mut`（clone-on-write），消除对 seeded-but-own-only 类型的 25×/run 假警报刷屏。

**现象（复现）：** 运行**任意** xtask 命令（`xtask --help` 即可）时，日志刷屏 25 次：
```
WARN try_fixup_inheritance: TypeDesc `Build.ProjectHooks` has additional Arc holders
     before fixup completed; cross-zpkg fields may be silently wrong
```
诊断实测 `Build.ProjectHooks` 在 fixup 时 strong_count = 4、weak = 0。

**根因（两层）：**
1. **为什么 ProjectHooks 会 own-only + 跨包 base**（构建侧，本 change *不*修 —— 见 Layer 2
   `fix-hooks-source-scan`）：xtask 项目在 `scripts/`，`[sources] include = ["**/*.z42"]` 递归把
   `scripts/hooks/hooks.z42` 扫进 xtask.zpkg，于是 xtask.zpkg 含一个 `Build.ProjectHooks :
   Z42.Build.BuildHooks`；而 xtask **无 `[dependencies]`**，`z42.build`（BuildHooks 所在包）不是
   构建期依赖、未被 merge → ProjectHooks own-only、base 跨包。
2. **为什么会刷屏假警报**（runtime，本 change 修）：`seed_types_for_lookup` 把 eager main-module 的
   TypeDesc **共享** Arc 进 lazy registry（strong_count ≥ 2），其不变式假设「eager 类型已 merge →
   `needs_fixup`=false → 永不 mutate」。ProjectHooks **违反**此不变式（eager 但 own-only、needs_fixup
   为真）。旧代码 `Arc::get_mut` 因共享失败 → **跳过 fixup + 每轮 WARN**。每次 lazy 加载一个 zpkg 都
   重跑一遍 fixup → 25 次刷屏。且警报文案「cross-zpkg **fields** may be silently wrong」是**假警报**：
   `BuildHooks` 零字段（只 6 个 no-op virtual），且 xtask 里的 ProjectHooks 是**从不实例化/派发**的
   死类（真实 hook 经 `[build] hooks` → z42b 单独编 `hooks/` → ModuleLoader.Load，count=1、fixup 正常）。

**修法：** `Arc::get_mut` → `Arc::make_mut`（CoW）。
- strong_count == 1（常态、真正惰性加载的类型）：等价旧 get_mut 快路径，in-place mutate、零行为变化。
- strong_count > 1（seeded-own-only 例外）：clone-on-write —— 给 lazy registry 一份私有、已 merge 的
  副本，lazy-lookup 路径拿到完整 vtable/fields、`needs_fixup` 下一轮为 false → 收敛、不再 warn。eager
  源 module 保留其 own-only 副本（不受影响；那是构建期限制，Layer 2 从源头去除该死类）。
- 需给 `TypeDesc` / `TypeDescCold` 加 `Clone` derive（字段本就全 Clone，零级联）。

- [x] 1.1 `type_registry.rs::try_fixup_inheritance`：`Arc::get_mut` match → `Arc::make_mut` CoW，删 WARN 分支
- [x] 1.2 `types.rs`：`TypeDesc` / `TypeDescCold` 加 `#[derive(..., Clone)]`
- [x] 1.3 `lazy_loader.rs::seed_types_for_lookup` docstring：补 own-only 例外 + CoW 兜底说明
- [x] 1.4 文档同步：`docs/design/runtime/vm-architecture.md` 阶段 2 fixup 节（get_mut→make_mut / 例外 / 幂等收敛）
- [x] 2.1 验证：`xtask --help` WARN 25→0；真实 hook 路径（BeforeCompile）照常 fired、0 WARN
- [x] 2.2 验证：`cargo test --release` 全绿（lib 1013 + 集成测）；改 runtime 跑全量非只 --lib

## 备注

- **只修 runtime 层（Q2：死代码不该报 warning）。构建侧根因（Q1：hooks 不该被扫进 app zpkg）另立
  change `fix-hooks-source-scan`**（涉 `z42.project` stdlib API + z42c，触发两-nightly 纪律，单独 DRAFT）。
- CoW 让 lazy 副本与 eager 副本**分叉**（lazy=merged、eager=own-only）。对 seeded-own-only 类型这是**期望**
  行为：真实派发/反射走 lazy 路径（`try_lookup_type`），拿到正确 merged 副本；eager own-only 副本本就是
  死的（该类从不经 main-module registry 派发）。normal 类型（needs_fixup=false）根本不进此分支，无分叉。
