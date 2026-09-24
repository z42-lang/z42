# Tasks: 包角色（role）与编译期扩展的解析域

> 设计 SoT：[design.md](design.md)｜动机：[proposal.md](proposal.md)。
> 每批单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| 批 | 内容 | bump | 状态 |
|----|------|:---:|------|
| 0 | 断 scripting 对 `z42.ir` 的假依赖：`FormatVersion` → z42i | 否 | ✅ 完成 |
| 1 | role 落地：`compiler-libs/` 解析域 + publisher 读 role + scripting 拆两包 + 门面搬家 | 否 | ⬜ |
| 2 | `kind="analyzer"` + `[analyzers]` 支持 `path` + 隔离校验 + handler ABI 握手 | 否 | ⬜ |
| 3 | 大重命名：`std.*` 用户库 + `z42c.*` 编译器域 | 否（跨 nightly）| ⬜ |
| 4 | `z42c.abi` 契约包 | 待评估 | ⏸️ 推迟 |

---

## 批 0 —— 断 scripting 的假依赖 ✅

**为什么先做它**：独立、纯收益、不阻塞讨论，且让批 1 的拆包干净（少一条要重新安置的依赖）。

- [x] 0.1 查证 `z42.ir` 在 scripting 里的全部用量 → **只有** `Script.FormatVersion()` 一处，
      为拼 `"zbc M.m, zpkg M.m"` 一句话（`ZbcVersion` + `ZpkgWriterZ` 两个编译期常量）。
- [x] 0.2 查证 `Script.FormatVersion()` 的调用点 → **唯一**：
      [interactive_main.z42:76](../../../../src/toolchain/interactive/core/interactive_main.z42#L76) 的 `.version` 元指令。
- [x] 0.3 `Script.z42`：删 `FormatVersion()` + `using Z42.Project` + `using Z42.IR.BinaryFormat`。
- [x] 0.4 `z42.scripting.z42.toml`：删 `"z42.ir"` 依赖 + 头注记由来与终局。
- [x] 0.5 z42i：新增 `_formatVersion()`（含终局注：该由 VM 自报）+ 两条 using + 清单加 `z42.ir`。
- [x] 0.6 GREEN：`build stdlib` 25/25 绿（scripting 断依赖后照常编过 = 假依赖坐实）；
      `build toolchain` z42i apphost ready；实测 `.version` → `zbc 1.44, zpkg 0.49`（与 Rust 侧
      `ZPKG_VERSION_MINOR = 49` 一致，零回归）。

> **不做**：不在批 0 引入 VM builtin 暴露格式版本（见 design 批 0 注）。那是 vm 类型完整变更流程，
> 不搭本批的车。批 0 是职责归位，不是终局。

### 批 0 踩到的坑（记账）

- `./xtask test stdlib` 在默认 toolchain（stable/1.88）下 **exit 0 但一个测试都没跑**，
  输出只有 22 行 cargo 版本抱怨（cranelift/wasmtime 要 rustc 1.95）。
  必须 `RUSTUP_TOOLCHAIN=1.98.1`。**这是一个假绿门**——exit code 不反映「什么都没跑」。
  单独记，不在本批修。

---

## 批 1 —— role 落地

- [ ] 1.1 `[project].role` 字段：`ManifestLoader` 解析（中性搬运，两值 `runtime`/`compile-time`，
      省略 = `runtime`）+ `ProjectInfo.Role`。
- [ ] 1.2 `compiler-libs/` 解析域：打包落点 + z42c 的 LibsDirs 分域（普通工程只看 `libs/`；
      `[analyzers]` 与 `role=compile-time` 工程的 `[dependencies]` 加看 `compiler-libs/`）。
- [ ] 1.3 `z42c.semantics` 进 `compiler-libs/`（**不**进 stdlib workspace）⇒ 外部 generator 终于可写。
      验收 = proposal §Why 里那个端到端复现从 `E0443 undefined type: ModuleGenerator` 变成编过。
- [ ] 1.4 `z42c.core` / `z42c.syntax` / `z42.ir` / `z42.project` / `z42.build` 标 `role=compile-time`，
      移出 `libs/`。
- [ ] 1.5 publisher 判据换成读 role（`builder_publish.z42`），退休两条打补丁的注释。
- [ ] 1.6 scripting 拆两包 + `IReplCompiler` 门面搬家（design §scripting ①②）。
- [ ] 1.7 `runtime` 包不再含编译器域包；发行形态按 design §scripting ③（**实施前最终确认**：
      新增 `z42-runtime-scripting` vs runtime 干脆不带 scripting）。
- [ ] 1.8 GREEN：全量 + `build sdk` → `test examples`（改发布布局 = 编译器可见行为，
      [[local-green-misses-examples-gate]]）+ `test packages`（staging 自检）。

> ⚠️ 风险：`_ensureBootstrapSelfDepLibs` 冷启动预建、`xtask_stdlib.z42` 的 `_stdlibList`、
> 扁平视图 hard-link 汇聚三处都按「stdlib workspace 成员」推导落点，1.4 会同时动到。

## 批 2 —— `kind="analyzer"` + 真依赖 + ABI 握手

- [ ] 2.1 `kind = "analyzer"` 第三种目标种类（现为 `exe|lib`）。语义四条：
      自动获得 compiler-api LibsDirs / 恒按宿主平台构建（不跟随目标 rid）/ 不进 `[dependencies]` 闭包
      与 publish payload / 双向校验。
- [ ] 2.2 `[analyzers]` 从「按名在 LibsDirs 找 `<name>.zpkg`」升级为与 `[dependencies]` 同构的
      DepEntry 解析（支持 `path`，优先 path → libs 兜底）。**这条是「用户自定义」从纸面变可用的关键。**
- [ ] 2.3 双向校验诊断：`kind="analyzer"` 的包出现在 `[dependencies]` → error；
      非 analyzer 包出现在 `[analyzers]` → error。诊断码按 diagnostic-code-uniqueness 规则分配
      （**逐个 `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`**，扫 main 不够）。
- [ ] 2.4 **handler ABI 握手 fail-fast**（裁决 ⑤ 的对冲，自批 4 提前）：`GeneratorLoader` /
      `AnalyzerLoader` 加载前校验 handler zpkg 与当前编译器同代，不同代 → 明确诊断而非崩。
- [ ] 2.5 退休 `KnownTestOnlyDeps = { "z42.test" }` 硬编码白名单——让包自己声明 kind/role。
- [ ] 2.6 端到端验收：在本仓库外建一个 `kind="analyzer"` 工程，主工程 `[analyzers]` 用 `path` 引用，
      跑出诊断 + 生成代码。**必须真跑，不接受"应该能行"。**

## 批 3 —— 大重命名

- [ ] 3.1 `std.*` 前缀：用户库改名（User 既定方向）。
- [ ] 3.2 `z42.ir` / `z42.project` / `z42.build` → `z42c.*`（**并进同一批**：重命名成本主要在种子纪律
      与引用点扫描，合并做边际成本远低于两次）。
- [ ] 3.3 support 先行、晚一个 nightly 再 use（[[bootstrap-seed]] 纪律）。
- [ ] 3.4 `src/libraries/README.md` 那段「两类库（别混淆）」脚注**删除**——它存在的理由被 role + 命名
      同时消灭。**这条是本批的验收信号**：补丁性散文能删掉，才说明机制真的替代了约定。

## 批 4 —— `z42c.abi` 契约包 ⏸️

推迟（裁决 ⑤）。形态稳定后再评估。已知代价：`GenTarget` 暴露 `Z42ClassType` / `SymbolTable`，
抽干净要么把这些一起下沉（拖出一大片），要么契约签名仍引用 semantics（等于没解耦）。
