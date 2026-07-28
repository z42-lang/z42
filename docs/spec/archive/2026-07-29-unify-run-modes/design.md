# Design: 运行时设置解析（P0）+ 多 exe 目标（P3–P5）

> 范围：本 design 覆盖 **P0（设置 SoT 收敛 + VM 端 `[runtime]` 解析）**（下文主体）与 **多 exe 目标（P3–P5，末节）**。P1（侧车迁移）/ P4 前门编排的细节各自实施时补。
> 父提案：[proposal.md](proposal.md)。

## Architecture

```
                    ┌──────────── 优先级链（端状态）────────────┐
   CLI flag  >  环境变量  >  [runtime] 配置文件  >  工程 profile  >  内置默认
   (main.rs)    (Z42_*)      (P0: 本 design)        (P2)            (KNOWN_KNOBS)
                    └── P0 实现这一段：env > file > default ──┘

   单一 SoT：KNOWN_KNOBS（config.rs）—— 每个旋钮声明 { env 名, TOML key, 默认, 消费者 }
                    │
   RuntimeConfig::resolve(env_getter, runtime_table)   ← P0 新增分层解析
                    │
   两条启动路径都汇入 z42vm → VM 端解析对 launcher / apphost 双路径同时生效
```

**为什么 VM 端解析**：`z42vm` 是唯一执行点，z42 launcher（读 JSON 侧车注入 Z42_*）和 apphost/z42-hostrun（只注 Z42_LIBS）两条路径最终都 exec 它。把 `[runtime]` 解析放 VM，两路径同时受益；且 runtime crate 已有 `toml = "0.8"`（`src/runtime/Cargo.toml:44`），零新依赖。若只放 z42 launcher，apphost 直起的 app 会漏配置。

## Decisions

### Decision 1：`[runtime]` 解析放在哪一层
**问题**：分层优先级链的"配置文件"环节谁来解析？
**选项**：
- A — **VM 端（Rust，`RuntimeConfig`）**：普适（launcher + apphost 双路径都覆盖）；runtime 已有 toml crate；但 VM 需知道"从哪个文件读"。
- B — z42 launcher（`Std.Toml`）解析后注入 Z42_*：复用现有 configProperties 注入机制；但 apphost 路径不经 launcher → 漏配置；且需把 KNOWN_KNOBS 的 env↔TOML 映射暴露给 z42 侧（跨语言重复 schema）。
**决定**：**A**。VM 是唯一汇聚点，解析放这里覆盖面最全、无跨语言 schema 重复、零新依赖。launcher（P1）退化为"决定 Z42_CONFIG 指向哪个文件 + 迁 JSON 侧车→TOML"，不再自己解析旋钮值。

### Decision 2：P0 从哪个文件读 `[runtime]`（避免与 P1 侧车迁移鸡蛋）
**问题**：侧车现在还是 JSON（P1 才迁 TOML），全局 `~/.z42/config.toml` 的定位又属 launcher/hostrun 领域。P0 若硬绑某个具体文件会与 P1 耦合。
**选项**：
- A — 新增指针旋钮 `Z42_CONFIG`（指向一个含 `[runtime]` 段的 TOML 文件路径）。unset → 不读文件 → 行为与现状**逐字节一致**（非破坏）。P1/P3 再决定谁设 `Z42_CONFIG` / 侧车自动发现。
- B — P0 直接自动发现 app 侧车：与 P1 的 JSON→TOML 迁移强耦合，破坏 P0 独立性。
**决定**：**A**。`Z42_CONFIG` 是"文件在哪"的指针（不是旋钮值），使 P0 在 runtime 单锁内完全自洽、可单元测试、非破坏。它自身登记进 KNOWN_KNOBS。

### Decision 3：KnobSpec 增 `toml_key` 字段——env↔TOML 映射的单一 SoT
**问题**：同一旋钮的 env 名（`Z42_GC_MODE`）与 TOML key（`gc-mode`）映射写哪？
**决定**：给 `KnobSpec` 加 `toml_key: &'static str`。KNOWN_KNOBS 成为 env↔TOML↔默认的唯一 SoT，`resolve` 与 `--info` 都读它。TOML key 用 kebab-case（与 `z42.toml` manifest 风格一致），去掉 `Z42_` 前缀并小写（`Z42_GC_MODE` → `gc-mode`）。

### Decision 4：修正 `Z42_GC_MINOR_THRESHOLD` 描述失真
**问题**：`config.rs:67-68` 描述为 "bytes of allocation before auto-trigger minor GC" / default_hint "64 KiB"，但字段 `gc_minor_threshold: f32` 实际默认 `0.75`、语义是**年轻代存活率阈值（0.0–1.0）**（`parse_gc_minor_threshold` 做范围校验，`gc/arc_heap.rs` 按比率消费）。属注册表描述与实现语义不符。
**决定**：改 `description` 为存活率语义、`default_hint` 为 "unset; defaults to 0.75 (survival ratio)"。实现前读 `parse_gc_minor_threshold` + `arc_heap.rs` 消费点确认精确措辞，不臆造。

### Decision 5：`Z42_TARGET` 登记为 reserved
**问题**：`config.rs:14` 注释称 `Z42_TARGET` "reserved" 但既不在 KNOWN_KNOBS 也无解析。
**决定**：登记进 KNOWN_KNOBS，`consumed_by: "reserved (not yet implemented)"`，`default_hint: "unset; reserved"`。登记使"预留旋钮"在 `--info` 可见、意图显式，不引入解析逻辑（保持 inert）。

## Implementation Notes

- **KnobSpec 排序不变式**：`config.rs` 有单测断言 KNOWN_KNOBS 按 name 字母序 + 无重复。新增 `Z42_CONFIG` / `Z42_JIT_PROFILE` / `Z42_TARGET` 须插到正确字母位。
- **`RuntimeConfig::resolve` 分层**：签名形如 `resolve(get_env: F, runtime_table: Option<&toml::Table>) -> Self`。每字段解析顺序：`get_env(name)` 命中即用；否则查 `runtime_table[toml_key]`；否则默认。保留现有 `from_env()` 作 `resolve(env, None)` 的薄封装（现有调用点不破）。
- **`Z42_JIT_PROFILE` 去 straggler**：`jit/lazy.rs:63` 现直接 `env::var("Z42_JIT_PROFILE").is_ok()`。改为读 `runtime_config().jit_profile: bool`，纳入分层。
- **`Z42_CONFIG` 加载**：`resolve` 前若 `Z42_CONFIG` 有值，`std::fs::read_to_string` + `toml::from_str::<toml::Table>` 取 `[runtime]` 段传入。文件不存在 / 无 `[runtime]` 段 → `None`（非致命，warn 级 log）。解析错误 → 明确 error（不静默吞）。
- **`--info` 输出**：枚举每旋钮 `name (env) | toml_key | default_hint | consumed_by`，并打印当前 `Z42_CONFIG` 生效路径（若有）。
- **非破坏保证**：`Z42_CONFIG` unset 且所有现有 env 行为不变 → `resolve` 结果与旧 `from_env()` 逐字段相等（单测覆盖）。

## Testing Strategy

- **单元测试**（`config.rs` 同级 `config_tests.rs` 或现有测试模块）：
  - `resolve(env, None)` == 旧 `from_env()`（非破坏）
  - env 命中优先于 runtime_table（优先级）
  - runtime_table 命中优先于默认
  - 三源皆缺 → 默认
  - KNOWN_KNOBS 仍字母序 + 无重复 + 每个 RuntimeConfig 路径字段仍在表内（现有不变式测试通过）
  - `Z42_GC_MINOR_THRESHOLD` toml_key 映射 `gc-minor-threshold`；范围校验不变
  - `Z42_CONFIG` 指向的 TOML `[runtime]` 段被正确读入；文件缺失 → None 且不 panic
- **Rust build**：`cargo build --manifest-path src/runtime/Cargo.toml --release` 无告警
- **GREEN gate**：`xtask test`（P0 不改 zbc/zpkg 格式、不碰入口行为，e2e/golden 应逐字节不变——作为非破坏的强证据）

## Out of Scope（P0 明确不做）
- CLI flag 层（`--mode` 等）改造：仍在 main.rs，P0 不动。
- 工程 `[profile.*].mode` 消费：P2。
- JSON 侧车 → TOML 迁移、`Z42_CONFIG` 由谁设置 / 侧车自动发现：P1/P4。
- launcher / apphost 端任何改动：P1+。

---

# Design: 多 exe 目标（P3–P5）

> spec：[specs/multi-exe-targets/spec.md](specs/multi-exe-targets/spec.md)。接回归档特性 [add-multi-exe-target](../../archive/2026-04-04-add-multi-exe-target/)（自举迁移时丢了消费逻辑）。

## Architecture

```
manifest [[exe]]  ──解析(已现成)──►  ProjectManifest.Exes[{name,entry,src}]
                                          │
   P3 build:  Main.z42  ExeCount==0 ─► 现有单入口路径（不变）
                        ExeCount>0  ─► for e in Exes:
                                          PackageCompile(entry=e.Entry, sources=e.Src‖[sources])
                                          → ZpkgWriter META entry=e.Entry
                                          → dist/<e.Name>.zpkg          （复用现有单入口烘焙链）
   P4 run:    launcher  --bin X ─► 跑 dist/X.zpkg（entry 已烤好，不覆盖）
                        无 --bin ─► default-run ‖ 报错列名
   P5 publish: 每 dist/<name>.zpkg ─► apphost（现有 per-zpkg 机制，payload 不改）
```

## Decisions

### Decision E1：多产物 = per-exe zpkg（非 lib+bins 去重）
**问题**：多 exe 如何映射产物？共享 `[sources]` 时代码是否去重？
**选项**：A — per-exe zpkg，每 exe 编一份（共享源集则各带一份编译产物，重复）；B — lib+bins，共享码编成一个 lib zpkg，exe 薄壳依赖它（无重复，但引入"包既 lib 又有依赖它的 exe"结构）。
**决定**：**A**。物理 1:1、复用现有单入口烘焙链（只把"调一次"变"循环调"）、非破坏（`ExeCount==0` 走原路径）。B 的去重留 future 优化——多数多-bin 工程体量小；exe 声明不相交 `src` 时本就不重复。

### Decision E2：run 选择靠产物 entry，不靠入口覆盖
**问题**：`z42 run --bin X` 怎么跑对入口？
**决定**：直接跑 `dist/X.zpkg`——其 META entry 已在 build 时烤成 `X.Entry`，VM 加载即用 baked hint（`main.rs:742`），**无需 CLI 入口覆盖**。入口覆盖（`z42vm <f> <Entry>`）保留作 zbc / 测试的既有用途，不进本路径。

### Decision E3：多 exe 默认选择 = default-run 否则报错
**问题**：多 exe 且无 `--bin` 跑哪个？
**决定**：读 `[project].default-run`；无则**报错列出所有 exe 名**（Rust 同款），不静默取第一个（避免"改了 exe 顺序就换了默认程序"的隐患）。

### Decision E4：publish 复用 apphost-per-zpkg，不改 payload
**问题**：每 main 一个可发 app 怎么产？
**决定**：per-exe zpkg 已各带 baked entry，故每个 `dist/<name>.zpkg` 直接走**现有** apphost-per-zpkg 打包（apphost payload 仍只需嵌 zpkg 路径，entry 在 zpkg 内），**零 payload 改动**。

## Implementation Notes
- **非破坏红线**：`ExeCount==0` 分支 = 现有单入口路径原样；z42c 自身源码无 `[[exe]]`，故自举不动点（gen1==gen2）必须保持——P3 GREEN 以此为强证据。
- **源集切分**：`e.Src`（SrcCount>0）时 discover 只该子集；否则复用 `[sources]` 的 discovery 结果。多 exe 各自 `PackageCompile` 一次（共享源集时可缓存 discovery，编译各出各产物）。
- **z42b 编排**：`builder_commands._orchestrate` 现为单 `Target`；多 exe 时遍历 `pm.Exes` 生成多个构建请求（或把"多目标"下沉进 z42c driver，z42b 只透传 —— 实施时二选一，倾向 driver 内循环，z42b 不必懂多目标）。
- **format（zbc vs zpkg）**：默认 `.zpkg`；`.zbc`（无依赖单编译单元）留作后续按 exe 推断的优化，本 change 一律 zpkg。

## Testing Strategy（多 exe）
- z42c 单测/e2e：双 exe manifest → 产两 zpkg，各 entry 正确；exe 专属 src 只编子集；`ExeCount==0` 产物不变。
- 自举不动点 gen1==gen2（关键非破坏证据）。
- launcher e2e：`run --bin` 选对产物；无 --bin+default-run；无 --bin 无 default-run 报错列名；--bin 名不存在报错。
- publish：双 exe → 两 apphost 各可独立跑。
- `examples/hello.z42.toml` 修为真可跑后纳入 e2e。
