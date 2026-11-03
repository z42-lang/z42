# manifest-targets

## 职责
清单声明式 test / bench / example 目标的端到端夹具。
每个子目录是一个真实工程，其 `z42.toml` 声明 `[[test]]`/`[[bench]]`/`[[example]]` 目标 +
`[tests]`/`[benches]`/`[examples]` 段，用来验证 xtask 的清单驱动发现 / harness 分派 /
退出码判定 / 三层 dev-dep 合并 / 具名过滤 / 显式覆盖同名 auto。**不**参与 golden regen/run、
不进 stdlib dist、不打包（golden 遍历只看 `src/tests/` 与各库的 `tests/`，不到这里）。

常规构建与执行 smoke 的断言放在工程旁的 `expect.toml`，与命令行夹具共用
[z42b 工具及占位符](../z42b/README.md#工具与占位符)。套件每轮重置并暂存整棵目录，源码树不写入产物。

## 功能索引
| 覆盖点 | 承载 |
|--------|------|
| harness=true 反射（[Test]） | `basic/tests/unit_ok.z42`（显式）+ `basic/tests/auto_conv.z42`（约定） |
| harness=false 退出码与子进程隔离 | `basic/tests/exit_ok.z42` 显式退出 0，`basic/expect.toml` 检查父 runner 的结果确认 |
| [[bench]] 反射（[Benchmark]） | `basic/benches/micro.z42` |
| [[example]] 编译门禁 + test=true 执行 | `basic/examples/hello.z42` |
| [[example]] test=false 只编不跑 | `basic/examples/custom/compile_only.z42`（省略 entry，误运行退出 93） |
| 显式覆盖同名 auto | `unit_ok` / `exit_ok` 目标名 == 文件 stem → 覆盖 auto 单元 |
| compile-then-test（`z42b test <toml>`） | `compile-then-test/expect.toml` 驱动纯测试工程构建，验证两个反射测试确实执行 |
| 零目标回退的 `--out-root` | 同一多步夹具清除旧产物后指定输出根，检查命名 zpkg 与工程旁零写入 |

## 如何测试验证
    xtask test toolchain builder            # 跑 [[test]] 目标（harness 两态）与 builder 夹具
    xtask test toolchain builder --target exit_ok  # 只跑名为 exit_ok 的目标（具名精确）
    xtask bench targets           # 跑所有 [[bench]] 目标
    xtask test                    # 全部阶段，含 example 编译与 test=true 执行

## 关联文档
- 设计：`docs/reference/src/toolchain/z42-toml.md` L5b（目标模型 / harness / exit-code / 约定发现）

## 核心文件
| 文件 | 职责 |
|------|------|
| `basic/z42.toml` | 声明四类目标 + 依赖，驱动发现 |
| `basic/tests/*.z42` | harness 两态 + 约定 auto 单元 |
| `basic/benches/micro.z42` | [Benchmark] 反射目标 |
| `basic/examples/hello.z42` | example（编译门禁 + test=true 执行） |
| `basic/examples/custom/compile_only.z42` | example（test=false，只编译） |
| `bundle-host/pass_unit.z42` | bundle 宿主执行用的 [Test] 单元 |
| `basic/expect.toml` | harness=false 的成功退出与父 runner 确认 |
| `compile-then-test/{z42.toml, src/Tests.z42, expect.toml}` | 多步构建、反射执行与 `--out-root` 产物隔离 |
