# Design: 编译器校验旋钮名

## Architecture

```
z42c build <toml>
  │
  ├─ ManifestLoader.Load → ProjectManifest { Profiles[{Name, Knobs[], Properties, BadKeys[]}] }
  │
  ├─ _validateProfileKnobs(pm)          ← 本 change 新增，在**编译前**
  │     ├─ BadKeys 非空            → error, exit BuildError   （旧形状，致命）
  │     └─ Knobs 的 key ∉ 登记表   → warning + 最近邻建议      （typo，不致命）
  │            登记表 = Std.Runtime.RuntimeConfig.Names() − 元旋钮
  │
  ├─ …编译…
  └─ _writeRuntimeConfigSidecar → dist/<name>.runtimeconfig.toml
```

旋钮登记表的 SoT 仍然**只有一处**——Rust 侧的 `KNOWN_KNOBS`。z42c 不持有副本，它在运行期
通过 `__cfg_names` builtin 问自己脚下这台 VM。

## Decisions

### Decision 1: 旋钮全集从哪来

**问题**：编译器要判断 `gc-mdoe` 是不是旋钮，得知道旋钮全集。而全集的 SoT 是 Rust 的
`KNOWN_KNOBS`（`src/runtime/src/config/knob_table.rs`），编译器是 z42 写的。

**选项**：

- A：把登记表移成一个 TOML 数据文件，Rust 与 z42c 各自读。
- B：Rust 端生成一份表给编译器读，加一道防漂移门（生成物与源表不一致即红）。
- C：**z42c 直接调 `Std.Runtime.RuntimeConfig.Names()`**。

**决定**：**C**。关键事实是 **z42c 本身就跑在 z42vm 上**——它进程里已经有一份活的登记表，
只是以前没人想到去问。A 要拆 SoT 并给 Rust 端加解析；B 要加生成物 + 防漂移门；C 零新增
数据文件、零复制、也就无从漂移。A 和 B 都是在解一个**不存在的**问题。

代价：z42c 报的旋钮全集是**构建机这台 VM** 的，不是目标机的。这正是 Decision 2 要处理的。

### Decision 2: 构建期查什么、不查什么

**问题**：`RuntimeConfig` 还能查「该旋钮在本 build/平台是否可用」（`IsAvailable`）、
知道它「哪几层可设」。要不要一并在构建期查？

**决定**：**只查"未知旋钮名"，其余留给运行时。**

- 未知名是**唯一**一类「构建机和目标机答案必然相同」的错——旋钮名不存在就是不存在，
  与 build feature、平台都无关。
- 「已知但不可用」**取决于目标机的 VM**。在带 jit 的机器上为 interp-only 目标构建是**合法**
  场景；构建机说 `jit-profile` 可用、目标机说不可用，两边都没错。构建期判这个必然误报。
- 「这一层不能设」对配置文件层只剩元旋钮一种情况，已并入"未知名"（见 Decision 4）。

### Decision 3: warning，不是 error

**决定**：未知旋钮名一律 **warning**，build 照常成功。

理由是**跨版本自举纪律的同一根轴**：用**旧工具链**为**新 VM** 构建时，新加的旋钮还不在旧
登记表里。判 error 会让"合法的旧编译器 + 新运行时"组合直接构建不了。warning 在这种情况下
只是一句可忽略的噪音，判红则是拦路。

与之相对，**旧形状**（`[profile.X]` 下直接写键）仍是 **error**：那是 manifest 结构错误，
与任何 VM 版本无关，且静默忽略会让迁移的人遇到"`mode = "interp"` 突然不生效且无提示"。

### Decision 4: 元旋钮不算合法配置文件键

`Names()` 对元旋钮（`Z42_CONFIG` / `Z42_APP_CONFIG` / `Z42_STRICT_CONFIG`）返回的是它们的
**环境变量名**——它们的 `toml_key` 是空的（`corelib/config.rs::public_key`），只收 cli+env
（写进配置文件会自指）。VM 的文件层查找只认 `toml_key`（`config/resolve.rs`），所以把
`Z42_CONFIG` 写进 `[profile.X.runtime]` 运行时照样报未知。

构建期按同一语义处理：**从全集里滤掉 `Z42_` 开头的名字**。元旋钮是登记表里仅有的大写对外名，
其余旋钮的 key 一律 kebab-case，判据稳定。

### Decision 5: 最近邻建议复用 `z42.text`，阈值对齐 VM

**问题**：只说"未知旋钮 `gc-mdoe`"帮助有限，typo 是主场景，应给出"是不是 `gc-mode`？"。

**决定**：距离函数用现成的 `Std.Text.Levenshtein.Distance`（`z42.text` 已在 driver 的传递依赖
里——`z42.io` 依赖它），**阈值逐字抄** `config/cli.rs::suggest_key`：编辑距离 ≤ 3 且不超过
key 长度的一半（向上取整、下限 1）。

同一个 typo 在 `--set` 和 manifest 里必须得到**同一句**建议；两处给不同答案比不给建议更让人
怀疑自己。手写一份 15 行的 Levenshtein 也能跑，但那是第二份实现 + 第二套阈值。

### Decision 6: 落点在 `_build` 早期，两个 profile 都查

- **早期**：manifest 的错不值得等一趟全量编译，fail fast。
- **两个 profile 都查**（不只当前构建的那个）：`[profile.release.runtime]` 的 typo 恰恰是
  "本地只跑 debug、发布时才炸"的形状。校验遍历 `pm.Profiles` 全部，与构建的是哪个 profile 无关。
- 顺带把 `BadKeys` 检查从 `_writeRuntimeConfigSidecar` 挪进来：它此前只在 `isExe` 时跑，
  **库工程的 `[profile.X]` 直接写键校验不到**。

## Implementation Notes

- `_validateProfileKnobs(ProjectManifest) -> int`（`ExitCode.Ok` / `ExitCode.BuildError`），
  在 `_build` 里紧跟 `ManifestLoader.Load` 之后调用。路径依赖闭包 / workspace 的每个成员各走
  一次自己的 `_build`，因此逐工程各校验一次。
- `ManifestLoader._profileKnobs` 只搬运**标量**键（非标量静默跳过），所以 `Knobs` 里每项都是
  `"key=value"`；按第一个 `=` 切 key。
- 启动开销：`Names()` 是一次 builtin 调用 + 33 个字符串；只在有 `[profile.*.runtime]` 键时才
  会走到 Levenshtein（33 次短串 DP）。相对一次 build 可忽略。

## Testing Strategy

`scripts/build/xtask_compiler_e2e.z42::_e2eKnobChecks`，挂在 `xtask test compiler` 的
e2e 段。三条断言，全部用 **kind=lib** 工程（不产侧车也照样校验，正是要证明校验不再挂在
侧车路径上）：

1. `gc-mdoe` → stderr 含 `gc-mdoe` 与建议 `gc-mode`，**且 rc==0**（不判红）。
   **不传 `--release`**——证明 debug 构建也查 release profile。
2. `mode` + `gc-mode` 全对 → stderr 无 `未知运行时旋钮`（防"每次构建都刷一行"的误报回归）。
3. `[profile.release]` 下直接写 `mode` → **rc != 0** 且报"不接受直接写键"（证明库工程也管）。
