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
tracked diff 与未跟踪文件的内容。执行时重新规划并核对整个计划；提交、配置或输入发生变化时拒绝旧计划。
提交模式还要求工作区干净。计划不携带可直接信任的任意 Shell，命令从当前配置重新推导。

`test changed [base]` 与 `test --changed <base>` 也读取同一配置，分别消费本地命令与 gate stage 元数据。
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
删除、排除或无法发现的成员保守回退。`docs` 标记普通文档快速通道，`publish` 标记发布输入，
`bench_only` 标记只需性能判定夹具、不需完整 A/B 测量的输入。

流程节点的字段：

| 字段 | 含义 |
|---|---|
| `command` | 本地 xtask 参数数组；空数组表示聚合节点 |
| `affects` | 该契约改变后受影响的流程，传递展开 |
| `requires` | 执行该节点前所需的流程，拓扑排序后执行 |
| `stages` | gate stage 键；`@z42` / `@all` 从现有 stage 注册表展开 |
| `outputs` | CI 选择信号，由规划器汇总 |
| `outside` | 完整 GREEN 未覆盖的义务，如 SDK 验证或完整 Runtime harness |
| `job` | CI job 名，默认节点 ID |
| `ci_command` | CI 消费已有产物时的参数数组，如 compiler 的 `--no-build` |

**执行前提不会反向扩散测试影响。** 例如 `tests -> runtime-test`，`runtime-test requires build-runtime`，
只补齐构建；即使 `build-runtime affects e2e`，也不会因此选中 e2e。只有改动影响图选中的节点才沿 `affects` 传播。
只需 CI 消费者的节点可以省略本地 `command`，通过 `ci_command` 和 `job` 保留独立 job 的验证义务。\n两张图分别拒绝环、未知引用和未知字段；未使用节点也须校验。稳定顺序与参数数组确保去重和含空格参数的准确传递。

`green` 节点执行完整 `test`，本地可覆盖已有窄测试命令；外部义务继续保留。被完整 GREEN 覆盖的节点仍保存在计划中，
CI 可以将其交给独立 job。SDK 验证、完整 Rust harness 和 benchmark 冒烟不会因选择 GREEN 而消失。

## CI 接入

`detect-changes` 先用种子 SDK 构建当前 xtask，不构建当前产品，再执行：

```bash
xtask check flows
xtask flow plan --event-file "$GITHUB_EVENT_PATH" \
  --out artifacts/ci-plan.json --github-output "$GITHUB_OUTPUT"
```

事件名与待验证提交来自 GitHub 环境。PR 使用事件的 base SHA，push 使用 before SHA；浅克隆先获取基线。
首次 push、无法获取或验证基线时回退全量并打印原因；schedule / workflow_dispatch 使用全量策略。
JSON 计划通过 `ci-plan` artifact 共享，host 测试用 `xtask test --plan` 消费 stage 选择，编译器 job 用：

```bash
xtask flow run --plan artifacts/ci-plan.json --job compiler-checks
```

`--job` 只运行分配给该 job 的选中节点及其前提，使用 `ci_command`；未知 job 报错。矩阵、runner、缓存、权限、
产物下载上传与 job 级依赖仍由 GitHub Actions 负责，YAML 不重复维护目录归属。

发布另用 nightly 到 HEAD 的累计差异，通过相同输入规则的 `publish` 字段判断；不能拿单次 push 差异替代。
`xtask ci supersede` 只取消没有未发布 SDK 输入的较早 main 运行。缺失 nightly、API 失败或 compare 达到 300 文件上限时
保留发布。它的 `sdk` 输出作为 `XTASK_RELEASE_REQUIRED` 传入计划，决定发布所需的打包范围。

## 验证与限制

`xtask check flows` 验证配置、影响与前提的隔离、workspace 归属、真实 Git 事件基线、重命名、计划过期、
发布范围、runner 恢复、上传重试、自举边界、golden 归档和安装器。它也作为完整 GREEN 的必跑 stage。
`xtask ci check-bench` 验证性能判定的入库夹具；`xtask ci check-installers book|wasm` 可单独运行安装器回归。

目录图描述显式契约关系，不代替编译器的语义依赖分析。未知路径保守全量；流程执行串行，CI 的并行由 jobs 和矩阵实现。
已有测试命令内部仍负责各自产物准备，配置可为新的组合流程声明额外 `requires`，无需复制其内部构建实现。
