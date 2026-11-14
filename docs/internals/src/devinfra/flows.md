# 按改动规划流程

> 代码：`scripts/flows.toml`、`scripts/src/flow/`、`scripts/src/ci/`、`scripts/src/test/changed.z42`
>
> 本地开发与 CI 使用同一个 xtask 规划器；配置描述目录归属、影响传播、执行前提和验证职责。

## 本地使用

```bash
./xtask flow plan                         # HEAD 到工作区，含暂存、未暂存与未跟踪文件
./xtask flow run                          # 执行同一计划，首个失败立即停止
./xtask flow plan --base main --merge-base # 分支分叉点到工作区
./xtask flow plan --base <sha> --head HEAD # 两个提交之间，排除工作区改动
./xtask flow plan --full                  # 完整策略
./xtask flow graph --format mermaid       # 输出目录与流程关系图
./xtask flow check                        # 校验配置并执行离线回归
```

`flow plan` 打印所选节点、命中规则和前置依赖；`--format json` 提供结构化结果。保存与执行可分离：

```bash
./xtask flow plan --out artifacts/local-plan.json
./xtask flow run --plan artifacts/local-plan.json
```

计划输出应放在 gitignore 覆盖的目录（如 `artifacts/`）或仓库外，防止计划文件自身变成新的改动。
计划绑定 schema、配置 SHA256、解析后的 base/head、事件策略及工作区摘要。工作区摘要包含索引状态、
tracked diff 与未跟踪文件的内容。配置在解析时将 CRLF 规范为 LF 后计算摘要，确保同一配置可跨 Linux / Windows checkout 消费。
工作区摘要仍保留原始内容校验；配置实质变更仍会使计划失效。执行时重新规划并核对整个计划；提交、配置或输入发生变化时拒绝旧计划。
提交模式还要求工作区干净。计划不携带可直接信任的任意 Shell，命令从当前配置重新推导。

`test changed [base]` 与 `test --changed <base>` 直接消费 `FlowPlan`：一次读取配置、一次规划，再分别取得本地命令或 gate stage 的补集。
配置按每次操作读取；workspace 归属缓存仅存活于本次规划中。Git 读取由 `common/git.z42` 共用，成员归属在 `flow/owners.z42`，stage 键从 GREEN 执行注册表取得。
开发增量验证不代替合并前的完整 `xtask test`。

## 配置与两种边

`scripts/flows.toml` 的 `version = 1` 标记 schema；`full` 是全量或未知输入时选中的根流程。
`[[inputs]]` 按文件顺序匹配，首个命中的规则拥有该路径；每个文件独立分类，混合改动取并集。
`include` / `exclude` 使用 `Z42.Project.PathGlob`：`*`、`?` 不跨目录，`**` 匹配零或多个目录段。
重命名以删除旧路径、增加新路径处理，删除的文件同样参与选择。

```toml
[[inputs]]
id = "runtime-private"
include = ["src/runtime/tests/*.rs"]
select = ["runtime"]
publish = false

[flows.runtime]
command = ["test", "runtime"]
stages = ["rust-units"]
outputs = ["runtime_checks"]
outside = true
```

`workspace` 指向现有 workspace 清单，`relative` 匹配成员内路径。成员及其排除规则复用项目发现器，
命令中的 `{owner}` 使用 `[project].name`，不从目录名猜测。`{chapter}` 用于 examples 的章节参数。
删除、排除或无法发现的成员保守回退。`docs` 标记普通文档快速通道，`publish` 独立标记发布输入。性能范围由流程节点的 `bench_stdlib`、`bench_vm`、`bench_integration` 信号决定，不再另列输入白名单。

流程节点的字段：

| 字段 | 含义 |
|---|---|
| `command` | 本地 xtask 参数数组；空数组表示聚合节点 |
| `affects` | 该契约改变后受影响的流程，传递展开 |
| `requires` | 执行所需的流程或资源节点；前提的 `outputs` 同样汇总，且不沿 `affects` 扩散 |
| `stages` | gate stage 键；`@z42` / `@all` 从现有 stage 注册表展开 |
| `outputs` | CI 选择信号，由规划器汇总 |
| `outside` | 完整 GREEN 未覆盖的义务，如 SDK 验证或完整 Runtime harness |
| `job` | CI job 名，默认节点 ID |
| `ci_command` | CI 消费已有产物时的参数数组，如 compiler 的 `--no-build` |

**执行前提不会反向扩散测试影响。** 例如 `tests -> runtime-test`，`runtime-test requires build-runtime`，
只补齐构建；即使 `build-runtime affects e2e`，也不会因此选中 e2e。只有改动影响图选中的节点才沿 `affects` 传播。
只需 CI 消费者的节点可以省略本地 `command`，通过 `ci_command` 和 `job` 保留独立 job 的验证义务。
两张图分别拒绝环、未知引用和未知字段；未使用节点也须校验。稳定顺序与参数数组确保去重和含空格参数的准确传递。

`green` 节点执行完整 `test`，本地可覆盖已有窄测试命令；外部义务继续保留。被完整 GREEN 覆盖的节点仍保存在计划中，
CI 可以将其交给独立 job。SDK 验证、完整 Rust harness 和 benchmark 冒烟不会因选择 GREEN 而消失。

## CI 接入

`detect-changes` 先用种子 SDK 构建当前 xtask，不构建当前产品，再执行：

```bash
xtask check flows
xtask flow plan --event-file "$GITHUB_EVENT_PATH" \
  --out artifacts/ci-plan.json --github-output "$GITHUB_OUTPUT"
```

规划器分别输出 `host_checks`、`toolchain_build`、`test_assets`、`docs_checks` 和 `archive_checks`。每个 job 只消费对应信号：有改动不等于要构建产品，有宿主验证也不等于要组装第二份 SDK。空 PR 不选择这些资源，只保留轻量规划与 `ci-ok`。无法读取基线、完整策略及非 PR 的累计发布义务保守保留前提。

| 改动职责 | 所选流程 |
|---|---|
| xtask 规划、命令路由、CI workflow | 种子 SDK 编译 xtask，`check xtask` 自身契约；VM benchmark runner 另用小型 Rust 夹具 |
| 标准库 / 编译器构建脚本 | 自身契约与对应 `build stdlib` / `build compiler`，CI 仅准备共享工具链 |
| VM 构建脚本 | VM harness、解释器 / JIT 验证和性能观测；不扩大为编译器或标准库单测 |
| 共享 unit harness | 宿主目标、stdlib / compiler 消费者与脚本 benchmark；不扩大为 VM、集成性能或设备打包 |
| toolchain 私有执行器 | 宿主 toolchain / manifest target 验证 |
| 发布归档与索引实现、归档自检 | 四平台种子 SDK 执行 `check release-archives`，覆盖所有 RID / workload 归档与真实解包；生产归档仍保留发布义务，选中 SDK 时由 SDK 验证覆盖 |
| 私有标准库测试 | 准备工具链，仅执行所属包；不测性能 |
| 私有 benchmark | 仅所属性能类别；不自动选择正确性 job |
| Android / iOS / wasm 执行器 | 仅对应平台编译与 smoke；共享设备实现仍保留全部平台契约 |
| VM / 编译器 / 标准库生产源 | 正确性消费者与对应性能类别；VM 生产源保留完整运行时和平台契约 |
| 无法识别的输入 | 完整策略，不以“脚本文件”作为自动免测依据 |

文档由 `check docs` 聚合链接、诊断码与布局检查：无宿主验证时复用规划器 job，有宿主验证时由宿主覆盖。归档契约是本地可直接执行的独立检查，完整 SDK 验证仍聚合相同夹具。

宿主 job 在本机准备工具链；`host-validation` 不额外申请供分片 / 编译器消费者下载的共享工具链产物。仅宿主验证的计划不会启动无消费者的 `compile-toolchain`，发布所需的准备仍由累计发布策略保留。

`test stdlib --plan <文件>` 和 `bench stdlib --plan <文件>` 在计划绑定的工作树中保留 `{owner}`，并与分片取交集；不会把单包选择扩大成全库。A/B 的 base、head 是不同工作树，各自在本树捕获完整的已选性能类别，不向另一棵树套用提交绑定的计划。

事件名与待验证提交来自 GitHub 环境。PR 从实际测试的合并提交读取两个 parent，验证第二个 parent 等于事件的 PR head SHA，再以第一个 parent 为基线。事件中的 base SHA 可能落后于合并提交，不能据此计算 PR 改动。浅克隆直接读取 commit 的 parent 头，再获取对应的不可变基线。push 默认使用 before SHA；main CI 通过 `ci main-scope` 取得最近成功的 main push CI，并以 `--base` 注入累计验证基线。`--event-file` 仅在 push 上允许覆盖 base，PR 基线不能被显式参数绕过。
首次 push、无法获取或验证基线时回退全量并打印原因；schedule / workflow_dispatch 使用全量策略。Bench 通过 `ci bench-scope --event-file` 复用同一规划器，输出 `base_sha` 供 A/B checkout、工具链构建与改动判断统一使用；无法建立比较基线时直接报错，避免比较错误的两棵树。
JSON 计划通过 `ci-plan` artifact 共享；消费 job 独立获取浅克隆中缺失的 base commit，再重新计算并核对计划。host 测试用 `xtask ci test-host --plan` 消费 stage 选择，编译器 job 用：

```bash
xtask flow run --plan artifacts/ci-plan.json --job compiler-checks
```

`--job` 只运行分配给该 job 的选中节点及其前提，使用 `ci_command`；未知 job 报错。矩阵、runner、缓存、权限、
产物下载上传与 job 级依赖仍由 GitHub Actions 负责，YAML 不重复维护目录归属。

发布另用 nightly 到 HEAD 的累计差异，通过相同输入规则的 `publish` 字段判断；不能拿单次 push 差异替代。
`ci main-scope` 分别输出累计验证基线与 `sdk` 发布信号；验证基线未知时工作流传入 `--full`，不会退回最后一次 push 的差异。`sdk` 作为 `XTASK_RELEASE_REQUIRED` 传入计划，决定发布所需的打包范围。缺失 nightly、API 失败或 compare 达到 300 文件上限时保留发布。main 合并队列与 PR 旧 head 清理的边界见 [CI 调度](ci.md)。

## 验证与限制

`xtask check flows` 验证配置、影响与前提的隔离、workspace 归属、真实 Git 事件基线、重命名、计划过期、
资源与三类 benchmark 选择、所属包和分片交集、发布范围、runner 恢复、上传重试、自举边界、golden 归档和安装器。它也作为完整 GREEN 的必跑 stage；选测与真实 Git 回归只由此 stage 执行一次。
`xtask bench check` 验证性能判定的入库夹具；`xtask ci check-installers book|wasm` 可单独运行安装器回归。

目录图描述显式契约关系，不代替编译器的语义依赖分析。未知路径保守全量；流程执行串行，CI 的并行由 jobs 和矩阵实现。
已有测试命令内部仍负责各自产物准备，配置可为新的组合流程声明额外 `requires`，无需复制其内部构建实现。
