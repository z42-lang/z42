# 开发基础设施

> 对齐：2026-09-17（change `restructure-docs-three-books`）

**在这个仓库里干活**要用的东西：怎么把环境搭起来、怎么构建、怎么跑测试与门禁、怎么打包发版、怎么排查。

这里混着两类页，按你的需要挑：

| 类型 | 回答 | 页 |
|---|---|---|
| **操作页** | 照着敲就能跑 | [开发环境准备](dev-setup.md) · [平台构建](build-platforms.md) · [怎么跑测试](testing.md) · [CI 拓扑](ci.md) · [发版流程](release.md) · [调试手法](debugging.md) |
| **机制页** | 为什么这样编排、在哪改 | [xtask](xtask.md) · [构建编排](build.md) · [GREEN gate](test-gate.md) · [测试流水线](test-pipeline.md) · [性能门禁](benchmarking.md) · [打包引擎](packaging.md) · [产物目录布局](artifacts-layout.md) |

另有一页是约定而非流程：[本仓命名与目录约定](repo-conventions.md)——改 z42 本身时用；
**用户代码的命名规则在参考手册**，那里才是 SoT。

> **与「测试体系」部分的分工**：本部分的 [怎么跑测试](testing.md) / [GREEN gate](test-gate.md) /
> [测试流水线](test-pipeline.md) 讲的是**仓库侧怎么组织和跑**；
> 测试框架本身（TIDX 语义、runner 协议、平台后端）在[测试体系](../testing/README.md)。
