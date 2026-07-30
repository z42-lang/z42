# z42c.pipeline

## 职责
镜像 C# [z42.Pipeline](../../compiler/z42.Pipeline/README.md)：编译管线编排（单文件 + 包级 Lexer→Parser→Sem→IR→Emit）。**B0 骨架：占位类型 `PipelineSkeleton`**（引用全部 5 个直接依赖，验证最深多依赖节点跨包编译）；真实编排 → 端到端 build 待 0.3.9。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/PipelineSkeleton.z42` | 占位（`namespace Z42.Pipeline`，引用 core/syntax/semantics/ir/project）|
| `src/DepScan.z42` | libs 目录扫描：DependencyIndex / nsMap / 跨包类型世界（prelude-first + Ordinal 排序）；`ScanDirsLazy` REPL 惰性路径——`LazyReconWorld` 按包懒填、基类链按 ns 路由只解析引用闭包（lazy-type-world，O(引用) 不随库总量增长）|
| `src/WorkspaceBuild.z42` | workspace 成员发现 + 拓扑序 + per-member 布局（WsPlan）|
| `src/IncrementalBuild.z42` | 文件级增量 probe（add-file-level-incremental）：`ProbeFiles` 种子（hash/条目·pin/包级源清单）+ `Close` token 保守边传递闭包（标识符 token ∩ 包内定义名）；`Z42_INCR_DEBUG` 种子+传播链；单测见 `tests/incremental/` |

## 入口点
`Z42.Pipeline`（命名空间）。

## 依赖关系
→ z42c.core, z42c.syntax, z42c.semantics, z42c.ir, z42c.project。stdlib 自动可用。
