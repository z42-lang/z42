# 第二部分 · 编译与构建（Compiler & Build）

z42 "编译相关"的全部：**z42c**（编译器本体，源码 → `.zpkg`）与 **z42b**（构建编排器，项目"编译 → 发布"全流程）。本部分讲两者的架构、数据流、产物格式、工具与诊断。

先读[架构总览](architecture.md)建立心智模型，再按需深入各章。

## 章节

| 章节 | 涵盖 | 状态 |
|------|------|:----:|
| [架构总览](architecture.md) | z42c 七子包 + z42b 编排器的分工、依赖图、两条主数据流、关键设计权衡 | ✅ |
| 源代码编译流程（z42c） | 单包：Lexer → Parser → AST → TypeCheck → IrGen → zbc | ⬜ |
| 工程模型与依赖解析 | manifest、依赖 DAG、DependencyIndex、TSIG 跨包符号导入、加载顺序确定性 | ⬜ |
| 工作区编译（z42c workspace） | 多包：DepScan → 拓扑排序 → 逐包编译 → ZpkgBuilder 组装 dist | ⬜ |
| 项目构建与发布编排（z42b） | 相位管线、ICompiler 进程内调 z42c、z42.build 框架 + WorkloadBase 继承链、launcher 分发 | ⬜ |
| 编译产物：zpkg / zbc 格式 | 产物布局、zpkg section、zbc 二进制（writer 视角） | ⬜ |
| CLI 与诊断工具 | z42c（build/compile/--workspace）+ z42b（build/publish/test…）+ `--dump-*` | ⬜ |
| 错误码体系 | Z#### 分段、Diagnostics vs Exceptions | ⬜ |
