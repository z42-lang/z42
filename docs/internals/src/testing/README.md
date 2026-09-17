# 测试体系

> 对齐：2026-09-17（change `restructure-docs-three-books`）

**测试框架本身**怎么实现的：用例怎么被发现、怎么被执行、跨平台怎么跑。

**怎么写测试不在这里**——`[Test]` / `[Skip]` / `Assert` / `z42 test` 的用法见
[语言与库参考的测试页](../../../reference/src/testing.md)。
**仓库侧怎么组织和跑**（GREEN gate 组成、CI 拓扑、`test changed` 映射）见
[开发基础设施](../devinfra/README.md)。

| 页 | 什么时候读 |
|---|---|
| [测试框架与 runner](framework.md) | 要改用例发现、执行、报告；要知道 TIDX 各字段谁写谁读 |
| [跨平台测试](cross-platform.md) | 要加一个平台后端，或改能力门控 |
| [嵌入式 app 运行](embedded-app-run.md) | 要改 test-agent / bundle 流水线 / 静动链接矩阵 |
| [执行 profile 矩阵](exec-profile-matrix.md) | 要加一根轴或改矩阵 schema |
