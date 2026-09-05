# scripts-batch3-naming — `scripts/` 命名归位：一名三义收敛 + 删 `_buildStdlib` shim

> 类型：`refactor`（最小化模式，无需 DRAFT 规范）。
> 属 scripts/ 结构优化程序**第 3 批**收尾（#472 文档漂移门 → #474 golden 枚举合一 →
> #480 拆 CLI → #485 handler 归位 → 本次）。

## 问题

三处名实不符，都在 `scripts/README.md` 里以「已登记、待后续重构 PR 正名」的形式挂了账：

### 1. `_buildStdlib` 是个 3 行 shim

`#485` 把它从 `xtask.z42` 搬到 `build/xtask_stdlib.z42`，就摆在 `_buildStdlibCore` 旁边——
`int _buildStdlib() { return _buildStdlibCore(); }`，零参数转零参数，没有任何适配。当时刻意
没删，因为删它要动 `_dispatchBuild`，而 #480 正把该函数搬进 `scripts/cli/`，两个 PR 会打架。
两者都合并了，账可以还。

### 2. `targets ↔ fixtures` 命名反向

| 文件 | 实际内容 |
|---|---|
| `test/xtask_test_targets.z42` | manifest target **引擎**，被 lib / fixtures / example 三个 flow 共用，**不对应任何命令** |
| `test/xtask_test_fixtures.z42` | `test targets` / `bench targets` 的**命令入口**（`_testTargetsCore`） |

找 `test targets` 的实现，会先撞上那个同名但其实是引擎的文件。

### 3. `xtask_test_*` 前缀一名三义

| 含义 | 文件 |
|---|---|
| ① `test <x>` 命令的实现 | `test/` 下多数 + `package/xtask_test_packages.z42` |
| ② `build test` 的实现 | `build/xtask_test_assets.z42` |
| ③ 某模块的 throw-on-mismatch 自检层 | `package/xtask_test_{packages_config,stage_components,package_assemble}.z42` |

②③ 里**没有任何 `[Test]`**（三个自检文件的头注释全在解释「xtask 是 exe，`[Test]` 反射
runner 看不见它，所以这里只能 throw-on-mismatch」）。`build/xtask_test_assets.z42` 更是
**编译** golden 资产的，自己一个测试都不含。

## 方案

### 定一条规则，写进 `scripts/README.md`

**`xtask_test_<x>.z42` 在全 `scripts/` 下只有一个含义：`test <x>` 命令的实现。**
引擎类不带该前缀；自检层统一 `xtask_selfcheck_<模块>.z42`。规则全表（四种角色 → 命名）
落在 README 的「文件命名规则」一节，替换掉原先那段「已登记待正名」的欠债提醒。

### 按规则改名

| 改动 | 从 → 到 |
|---|---|
| shim | 删 `_buildStdlib` shim，`_buildStdlibCore` **改名** `_buildStdlib` |
| 引擎 | `test/xtask_test_targets.z42` → `test/xtask_manifest_targets.z42` |
| 命令入口 | `test/xtask_test_fixtures.z42` → `test/xtask_test_targets.z42` |
| `build test` | `build/xtask_test_assets.z42` → `build/xtask_golden_assets.z42` |
| 自检层 ×3 | `package/xtask_test_<m>.z42` → `package/xtask_selfcheck_<m>.z42` |

**shim 为什么是「改名 Core」而不是「改调用点」**：`_buildStdlib` 有 12 个调用点、
`_buildStdlibCore` 只有 shim 一个。而且 `Core` 后缀在本仓的语义是「有薄 wrapper 的实现体」
（`_testLibCore` / `_buildPackageCore` / `_regenCore` 都吃参数、由更薄的入口包着）——
wrapper 一删，后缀就没了意义。

### 刻意没改的

- **`package/xtask_package_test.z42`**（打 test workload）：它与 `_desktop` / `_ios` /
  `_android` / `_wasm` 是同一族，`test` 是 **workload 名**不是「测试」，改名反而破坏族内对称。
  README 就地注明这层含义。
- **`package/xtask_test_packages.z42`**：它就是 `test packages` 命令的实现，本来就合规。
- `docs/spec/archive/` 与已完成 change 的 `tasks.md` 里的旧文件名：那些是**历史记录**，
  记的是当时的事实，不回填。

## 验证

1. **编译期即证**：namespace 扁平（`Z42Xtask`）+ `include = ["**/*.z42"]` + 跨文件裸名互调 →
   改名文件零成本，但**函数**少一个就是 `E0401 undefined`。`z42c build scripts/xtask.z42.toml
   --release` 0 错误 = `_buildStdlibCore` 的调用点确实只有那一个 shim、且全部 12 个
   `_buildStdlib` 调用点仍解析得到。
2. **`test changed` 不受影响**：`_mapFile` 对 `scripts/` 前缀一律 `_mapFull()`（全量），
   不按具体文件名分派 → 改名不改 stage 选择。
3. **CI path filter 不受影响**：`.github/workflows/ci.yml` 只列了
   `scripts/test/xtask_test_{platform,wasm,ios,android,desktop}.z42`，均未改名。
4. `xtask test` 全绿 **10/10 stage**（4m06s；gate 本身跑遍 `build stdlib` / `build test` /
   manifest-targets / examples 这几条被改动的路径）。
5. **补跑 opt-in 的 `xtask test packages`**（三个自检层就是本次改名的文件，而它**不在**
   GREEN gate 内 → 光靠 `xtask test` 覆盖不到）：`packages-config` / `packages-staging` /
   `packages-assemble` 三层全 PASS，rc=0。

## 状态

🟢 完成
