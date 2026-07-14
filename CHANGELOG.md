# Changelog

## v0.3.0（2026-07-13）— 编译器全自举，C# 种子退役

> v0.2.0 → v0.3.0：36 天，855 个 commit，1608 个文件变更，+94k / −72k 行。

### 🏆 招牌：编译器全自举（Self-Hosting）

- **编译器 7 个子系统（core / syntax / semantics / ir / project / pipeline / driver）全部用 z42 重写**，
  z42 编译器现在由 z42 自己编译自己。
- **byte-identical 门禁**：自举编译器编译自身 7/7 包，产物与原 C# 编译器逐字节一致，
  作为 CI 常驻 gate（`xtask test compiler-z42`）持续守护。
- **C# bootstrap 编译器整体删除**（`src/compiler` C# 代码清零）：构建、测试、打包、
  golden 回归全链路不再依赖 dotnet —— 从源码构建 z42 不再需要安装 .NET SDK。
- **自举鸡蛋问题工程化解决**：nightly 种子分发 + 冷启动统一 resolver（`Z42_HOME`）+
  两代自举（Gen1/Gen2）根治 zbc/zpkg 格式 bump 时的引导死锁；确立「新语法 support 先行、
  晚一个 nightly 再 use」的跨版本自举纪律，上一版编译器永远能编当前源码。

### 🔤 语言与编译器能力

- 自举编译器补齐全语言面（golden parity 96 → 130 全通过）：闭包 / lambda、泛型方法与
  where 约束、接口 dispatch、try/catch/throw、运算符重载、`?.` 空条件访问、switch 表达式、
  records、索引器、delegate + 方法组转换、局部函数、插值字符串、ref/out 等。
- 新语言特性：`params` 变长参数（含重载决议）、`event` 多播字段、基于类型的方法重载决议、
  自定义 attribute（类 / 字段 / 方法 / 参数四级）、原始类型 ↔ object 装箱拆箱。
- **文件级增量编译**（cache 为 SoT，dist 为确定性投影）+ 单工程增量（.zbc 落盘 + 整包 probe）。
- 二进制格式演进至 **zbc 1.22 / zpkg 0.26**：STRS 段字典重编码、类型元数据统一
  （删 TSIG/EXPT，TYPE/SIGS/IMPL 成唯一元数据源）、enum / 参数 / 可见性 / 方法修饰符入段、
  indexed zpkg 支持最小 patch 分发。

### 🪞 反射：从零到 MVP+

- `Std.Type` + `FieldInfo` / `MethodInfo` / `ParameterInfo` 完整成员层级，`typeof(T)` 返回真句柄。
- 非泛型 `MethodInfo.Invoke`、`Activator.CreateInstance(Type)`、`Type.GetType`。
- 类型谓词与泛型反射：`IsClass` / `IsInterface` / `IsValueType` / `IsAbstract` / `IsPrimitive`、
  `GetInterfaces`（含跨包 impl trait）、`GetGenericArguments` / `GetGenericTypeDefinition`、
  `GetElementType`、`IsAssignableFrom`。
- 落地应用（吃自己狗粮）：**Rust 写的 test-runner 被删除**，替换为纯 z42 的反射式
  TestRunner（`Std.Test.Runner`，经 `z42b` 驱动）。

### 🧰 工具链与分发

- `z42` launcher / `z42b` / xtask 全部改为原生 apphost 可执行文件，删 trampoline crate。
- 发布拆为 **SDK / launcher / runtime 三包** + `release-index.json` 清单；
  `install-z42` 一键安装、`z42 install / self-update` 联网自更新。
- **workload 体系**：desktop / iOS / Android / WASM 平台导出（`z42 publish` / `z42 export`）+
  `z42 workload install / list / uninstall`，多 RID 增量安装。
- **VSCode 语法高亮扩展**（IDE 支持 A 期）+ `z42c --dump-keywords`。
- CI 三平台模拟器真机测试管线：WASM / iOS Simulator / Android Emulator 统一跑平台测试。
- xtask 全命令 MSBuild 风格 5 级 `--verbosity`；`Std.Cli` 嵌套子命令路由（launcher / xtask 已迁移）。

### ⚡ 性能与 VM

- **z42vm 默认执行模式 interp → JIT**。
- JIT 攻坚：调用帧零中间分配、malloc churn ~8×↓、字符串 Length/CharAt 元数据缓存修复
  lexer O(n²) —— 自举编译 z42.core 从 8.2s 累计降至 3.8s（>2×）。
- 跨包 Call 目标按 site 缓存；JIT eager 加载 transitive BFS + 单函数 interp 降级兜底。
- CI 提效:JIT 一致性套件 4 机分片（~58min → ~24min）、golden 并行再生、编译产物跨 job 复用。

### 📚 标准库与加固

- 新增：`Std.Runtime`（LoadZpkg / CallStatic 动态加载）、`Math.Sign` / `Clamp`、
  `Char.IsDigit` / `IsLetter`、`String.ToCharArray`、集合家族 `ToArray` / `AddRange`、
  `Stream.ReadByte` / `WriteByte`、`Directory.Copy`、`File.GetLastWriteTime`、
  `IPAddress` / `Uri` IPv6 解析、`BinaryReader/Writer` IEEE-754 浮点序列化。
- 加固：JSON / TOML parser 递归深度上限（防栈溢出 DoS）、TOML 数字扫描收紧、
  JSON 大整数溢出回退 f64、ISO-8601 按月长校验日、`Double.CompareTo(NaN)` 全序、
  `String.Substring` 越界检查。

### 🧹 内部质量

- TypeChecker / Parser / IrGen 三个 God-Class 全部拆分（MemberResolver / StmtBinder /
  ExprTyper / TypeParser / ExprParser / StmtParser / DeclParser / …）。
- 统一 safepoint/STW 协议与精确 GC 契约设计落档，loom 模型测试为并发 GC（0.3.x 后续）铺路。
