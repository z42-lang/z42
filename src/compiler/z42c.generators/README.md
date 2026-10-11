# z42c.generators

## 职责
编译期 **generator**：在编译时**生成或改写源码**——给被 `[X]` 标注的声明增补成员 / 替换声明（applied generator），或扫描全编译产出聚合源码（module generator，如路由表、DI 容器、serde 注册）。本包提供 generator 的**契约**、外部 generator zpkg 的**加载**、**多轮引擎**，以及语言内建的 `[Forward]`。

在包编译里的位置是**前置线 ③**（真绑定之前）：引擎先做一次 provisional 绑定让 generator 能解析符号，产出合并成 union CU，再交给主编译线**只跑一遍**——生成的代码与手写代码走同一条 收集 → 类型检查 → 代码生成 → 优化。generator 不碰 IR。见 [source-compile.md](../../../docs/internals/src/compiler/source-compile.md)。

**不做**：永远开启的内建 AST 脱糖（attribute 工厂合成、`[Benchmark]` 脱糖）在 `z42c.semantics` 的 `Desugar/`；只读诊断在 [`z42c.analyzers`](../z42c.analyzers/)。

## 功能索引
命名空间 `Z42.Generators`。

| 功能 | 入口 |
|------|------|
| 契约 | `Generator`（applied：`AppliedName()` + `Generate(GenTarget, GenSink)`）/ `ModuleGenerator`（`Generate(GenContext, GenSink)`）/ `GenSink`（`AddSource` / `Augment` / `Replace`）/ `GenTarget` / `GenContext`（`TypesWith<T>` / `MethodsWith<T>` / `Resolve`）（`src/Generation.z42`）|
| 外部 zpkg 加载 | `GeneratorLoader.Load(zpkgPaths, n) → LoadedGenerators`（与 analyzer 共用 `[analyzers]` 段的同一批 zpkg）|
| 多轮引擎 | `GeneratorDriver.RunRounds(...) → GenOutcome`：按 `Consumes()` / `Produces()` 拓扑分层（`GenTopo`，成环 E0449），逐层 provisional 绑定 → 运行 → 三种 sink 落地；护栏 E0448 |
| 内建 `[Forward]` | `ForwardGenerator`（成员转发；按需注册：`AnyTrigger(cus)` 预扫命中才加入）|

## 基础用法
写一个 generator（作者侧）见 [compile-time-extensions.md](../../../docs/reference/src/toolchain/compile-time-extensions.md)。编译器侧由 `z42c.pipeline` 的 `PackageCompile` 调用：

```z42
GenOutcome ro = GeneratorDriver.RunRounds(gens, gn, mods, mn, cus, files, count, imported, diags);
if (ro.HasOutput) { cus = ro.Cus; /* union：原始 CU 在前、生成 CU 在后 */ }
```

## 如何测试验证
```bash
./xtask test compiler   # tests/generator（applied / module / 多轮 / 字段触发）；外部 zpkg 端到端在 z42c.pipeline/tests/pkgcompile
```

## 关联文档
- 包编译里的位置（前置线 ③、双次绑定）：[source-compile.md](../../../docs/internals/src/compiler/source-compile.md)
- attribute 三路分流（generator 触发名不走 store-meta）：[attribute-pipeline.md](../../../docs/internals/src/compiler/attribute-pipeline.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Generation.z42` | 契约：接口 + 结果类 `GenType` / `GenMethod`（无 delegate）；类名须 `*Generator`（E0447）|
| `src/GeneratorDriver.z42` | 引擎：applied `Run` / module `RunModules` / 多轮 `RunRounds`；Augment = 合成 partial 碎片（继承宿主 using）、Replace = 按 DeclId 换 AST、AddSource = 新 CU；ordering fixup（剥触发 attr）|
| `src/GenContext.z42` | module generator 的查询面实现（`typeof(T)` + 剥 `Attribute` 后缀扫 AST）|
| `src/GenTopo.z42` | consumes / produces 拓扑分层（Kahn 最长路径）|
| `src/ForwardGenerator.z42` | 语言内建 `[Forward]`：按字段类型的成员面生成转发方法 |
| `src/GeneratorLoader.z42` | 外部 zpkg 两路加载：反射投影发现 `: Generator` / `: ModuleGenerator` + `ModuleLoader.Load` 使可执行 → 实例化 |

## 依赖关系
`z42c.core`、`z42c.syntax`、`z42c.semantics`（`SymbolCollector` / `SymbolTable` / `Z42ClassType` / `ImportedSymbols` / `HandlerRegistry`）。被 `z42c.pipeline` 依赖。
