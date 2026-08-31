# corelib/

## 职责

z42 VM 的**原生 builtin 层**：为 z42 标准库（`Std.*`）里那些无法用纯 z42 表达、
必须落到 Rust 的能力（IO、文件系统、反射、进程/线程/网络、加密、GC 控制…）提供
native 实现。每个 builtin 是一个 `fn(&VmContext, &[Value]) -> Result<Value>`，
统一登记进 mod.rs 的 `BUILTINS` 表（单一真相源），由解释器 `Instruction::Builtin`
和 JIT `jit_builtin` 两条路径经 `exec_builtin_by_id` 调用。

**不做**：语言语义（在 `interp` / `jit`）、类型/字节码元数据（在 `metadata`）、
GC 堆本身（在 `gc`）。凡能用纯 z42 写的（StringBuilder / List / Assert 等）都已从
这里下沉到 stdlib 脚本，corelib 只保留真正需要 Rust 的最小面。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| builtin 注册表 + 名称↔id 解析 + 分发 | `mod.rs` 的 `BUILTINS` 表 / `exec_builtin_by_id` |
| 值转换 / 解析 / to-string | `convert.rs` |
| 控制台 IO（print/readline/concat/len） | `io.rs` |
| 字符串操作 / 字符 | `string.rs` / `str_meta.rs` / `char.rs` |
| 数学 | `math.rs` |
| 文件系统 / 路径 / env / 时间（平台隔离后端） | `fs.rs` + `fs_backend.rs` |
| 对象内建（GetType / RefEq / HashCode） | `object.rs` |
| **反射**（`Std.Type` / `Std.Reflection.*`：枚举成员、attribute、反射调用、Activator、模块加载） | **`reflection/`**（子目录，见下「核心文件」） |
| 值类型 struct 字段布局复现（反射读写内联/装箱 struct） | `struct_reflect.rs` |
| 程序集加载上下文（`AssemblyLoadContext`） | `assemblyloadcontext.rs` |
| 运行时诊断 / 计数器 / profile | `diagnostics.rs` |
| 数组内建 | `array.rs` |
| GC 控制（collect / 阈值 / 快照） | `gc.rs` |
| 基准计时 | `bench.rs` |
| 进程 / 平台 / 系统信息 | `process.rs` / `platform.rs` / `system.rs` |
| 线程 / 锁 / 锁争用探针 | `threading.rs` / `sync.rs` / `sync_contention.rs` |
| 网络 / TLS / 加密 | `network.rs` / `tls.rs` / `crypto.rs` |
| REPL 支持（求值宿主 / 行编辑） | `repl.rs` / `repl_editing.rs` |
| 测试宿主（隔离跑 golden） | `tests.rs`（+ `reflection/module_load.rs` 的 `__run_goldens_isolated`）|

## 基础用法

**加一个 builtin**（新增一条需要 Rust 的 native 能力）：

1. 在对应类别文件里写 `pub fn builtin_foo(ctx: &VmContext, args: &[Value]) -> Result<Value>`
   （文件超 500 行硬限时按职责拆子目录，参照 `reflection/`）。
2. 在 `mod.rs` 的 `BUILTINS` 表**追加一行** `("__foo", 模块::builtin_foo)`
   —— **只能追加、不能插入中间**：表内位置就是稳定的 `BuiltinId`（进程内不变）。
3. z42 侧用 `[Native("__foo")]` 声明对应外部函数（stdlib）。

**lenient 约定**：类型/反射类 builtin 对「无 handle 的合成 Type（数组 `T[]`，及 z42.core
未加载时的 primitive 兜底）」或非预期入参一律返回空数组 / null，不 `bail!`（镜像 C# 返回空结果）。
（fix-type-reflection-names 起 primitive 正常解析为真 `Std.*` 句柄，不再走合成——`typeof(int)` ≡ `(5).GetType()`。）

## 如何测试验证

```bash
# corelib Rust 单元测试（各 <mod>_tests.rs，含 reflection/reflection_tests.rs）
cargo test --lib --manifest-path src/runtime/Cargo.toml

# 反射等端到端 golden（src/tests/reflection/ 等）
xtask test e2e

# stdlib [Test] dogfood（Std.Reflection / Std.IO / … 真实调用 builtin）
xtask test stdlib
```

## 关联文档

- 设计/机制（深入层）：[反射机制](../../../../docs/design/language/reflection.md)、
  [运行时 IR](../../../../docs/design/runtime/ir.md)
- 反射能力的引入/演进：`docs/spec/archive/2026-06-*-add-reflection-*` 等（需求↔迭代可追溯）
- 拆分：change `refactor-reflection-split`（`reflection.rs` 2840 行 → `reflection/` 11 子模块，全 <500）

## 核心文件

| 文件 | 职责 |
|------|------|
| `mod.rs` | `BUILTINS` 单一真相源表 + 名称↔`BuiltinId` 解析 + `exec_builtin_by_id` 分发 + `NativeFn` 类型 |
| `reflection/mod.rs` | 反射 hub —— 导入 + 共享常量 + 私有 re-glob 各 concern 子模块（兄弟经 `use super::*` 互见）+ `pub use` 保留公开 API |
| `reflection/type_object.rs` | `Std.Type` 对象构造（`make_type_object` / `make_type_from_name` / `make_constructed_type`）+ handle/slot 辅助 |
| `reflection/type_query.rs` | 类型查询谓词（base / interfaces / members / nested / is_abstract·sealed·value·record·interface·class·primitive·generic / assignable_from / visibility）|
| `reflection/fields.rs` | 字段枚举（`__type_fields` + `FieldInfo` 构造，含继承静态字段）|
| `reflection/methods.rs` | 方法/构造函数枚举（`__type_methods` / `__type_constructors` + `MethodInfo`/`ParameterInfo` 构造 + 签名解析）|
| `reflection/properties.rs` | 属性枚举（`get_`/`set_` 访问器归并为 `PropertyInfo`）|
| `reflection/attributes.rs` | custom-attribute 反射（type/method/field/property/param → 调工厂函数实例化）|
| `reflection/generics.rs` | 运行期泛型实例化（`MakeGenericType` + `where` 约束校验）|
| `reflection/enums.rs` | 枚举反射（names/values/name/parse/is_defined/underlying）|
| `reflection/invoke.rs` | 反射调用（`Method.Invoke` / `MakeGenericMethod` / `Activator.CreateInstance` / `Ctor.Invoke` / `__invoke_static`）+ slot 读取 |
| `reflection/accessors.rs` | 反射读写字段/属性值（含内联/装箱 struct 的 byte-region 读写 + 写屏障）|
| `reflection/module_load.rs` | 运行期加载模块/字节码（REPL、test 宿主）+ `__run_goldens_isolated` |
| `convert.rs` / `io.rs` / `string.rs` / `math.rs` / `fs.rs` | 各类别 builtin 实现（见「功能索引」）|
| `struct_reflect.rs` | 值类型 struct 字段布局复现（供 `reflection/accessors.rs` 读写内联/装箱 struct）|
| `<mod>_tests.rs` | 各模块 Rust 单元测试（`reflection` 的在 `reflection/reflection_tests.rs`）|
