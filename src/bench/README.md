# src/bench/ — 整程序性能场景

## 职责

`xtask bench` 测量的整程序场景（VM 启动 + stdlib 加载 + 执行的 wall-clock），以及它的能力探针、
结果 schema 与判红逻辑自检 fixture。只计时、不判对错——正确性用例在 [src/tests/](../tests/)；
单函数微基准跟着被测代码走（各库的 `bench/`、[src/runtime/benches/](../runtime/benches/)）。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 计时场景（头注释声明 `// tier: gate\|full`，可选 `// requires-caps:`） | `scenarios/<NN>_<name>.z42` |
| 能力探针：报告被测 VM 的 `Capabilities()` / `ExecModes()` | `probe/capabilities.z42` |
| 结果 JSON 的 schema（`xtask bench` 产出、`--diff` / `--ab` 消费） | `baseline-schema.json` |
| 判红逻辑自检 fixture（`bench-pr.yml` 在测量前先跑） | `testdata/*.json` |

## 基础用法

```bash
xtask bench                 # 全部场景，默认 jit
xtask bench --tier gate     # 只跑 PR 门禁那几条
xtask bench --quick         # sanity：前 2 个场景、runs=3，< 1 分钟
```

## 如何测试验证

```bash
xtask bench --quick         # 场景还能编、能跑：每条打印 mean ± σ，退出码 0
```

判红规则、阈值与「加一条 scenario」的步骤见[性能基准与回归门禁](../../docs/internals/src/devinfra/benchmarking.md)。

## 关联文档

- 判红语义与 CI 门禁：[性能基准与回归门禁](../../docs/internals/src/devinfra/benchmarking.md)
- 能力探针与 profile 矩阵：[执行 profile 矩阵](../../docs/internals/src/testing/exec-profile-matrix.md)
- 放在这里而不是 `src/tests/` 或 `src/runtime/` 的理由：[测试用例组织规范](../../docs/internals/src/devinfra/test-layout.md)
