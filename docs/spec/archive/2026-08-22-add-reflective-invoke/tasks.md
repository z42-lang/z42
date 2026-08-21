# Tasks: 反射式调用补全（泛型方法 Invoke + 构造函数反射）

> 状态：🟢 已完成 | 创建：2026-08-21 | 完成：2026-08-22 | 分支：add-reflective-invoke | worktree：../z42-genreflect

## 进度概览
- [x] 阶段 1: 方法级泛型元数据 producer（IrFunction + FunctionEmitter + ZbcWriter；**无格式 bump**）
- [x] 阶段 2: 泛型方法反射 API + native（type_params 露出 + MakeGenericMethod / Invoke 线程）
- [x] 阶段 3: 反射层级 + 构造函数反射（MethodBase / ConstructorInfo / GetConstructors / ctor Invoke）
- [x] 阶段 4: 测试（generic-method-invoke 2/2 + ctor-reflection 2/2 golden 全绿）
- [x] 阶段 5: 验证 + 文档同步（自举 5/5 字节不动点 + e2e 512/11/1 + stdlib 0-failed + vscode-syntax + cargo 958 全绿）

> **实施期改写**：原「阶段 2 格式 bump」**取消**——zbc SIGS 段已预留方法类型形参槽（reader 全链路已读，writer 恒写 0），
> 填真实值不改布局、现有源零泛型方法 → 无版本 bump/fixture/两代自举。详见 design.md「数据来源」。

## 阶段 1: 方法级泛型元数据 producer（无格式 bump）
- [x] 1.1 `IrModule.z42`：`IrFunction` 加**方法级** `string[] TypeParams` + `int TypeParamCount`（默认空/0；构造器不加必填参，沿用旧 ABI 惯例，构造后赋值）
- [x] 1.2 `FunctionEmitter.z42`：把 `md.TypeParams.Names`/`.Count`（方法级，非 class-merged）填入 IrFunction 新字段
- [x] 1.3 `ZbcWriter.z42`：intern pre-pass 加方法类型形参名（镜像类 `:89`）；`:443` 硬编码 `WriteU8(0)` 换成真实 `tpCount + 每 tp(nameIdx + cflags=0 + ifaceCount=0)`（照抄类 writer `:269-290`，where 约束 Deferred）

## 阶段 2: 泛型方法反射 API + native（按 pipeline：runtime → stdlib API）
- [x] 2.1 `bytecode.rs`/`merge.rs`（按需）：确认运行期 `Function` 是否携带 type_params；未携带则加字段 + thread `FuncSig.type_params`→`Function`
- [x] 2.2 `reflection.rs`：`resolve_func_sig` 返回 type_params；`build_method_info` 据此填 `IsGenericMethod`/`IsGenericMethodDefinition` + 类型形参名槽
- [x] 2.3 `reflection.rs`：`__method_make_generic(mi, typeArgs)` native——arity/泛型性校验（非泛型或 arity 错 → catchable `Std.Exception`）+ 克隆 MethodInfo 盖 `__typeArgs`
- [x] 2.4 `reflection.rs`：`builtin_method_invoke` 读 `__typeArgs`→转 FQ 名 `Box<[String]>`→线程进 `invoke_qualified`
- [x] 2.5 `reflection.rs`：`invoke_qualified` 加 `method_type_args` 参 → 填帧 exec 变体（复用 `exec_function_from_regs` 填帧模式；空切片=byte-identical 非泛型）
- [x] 2.6 `corelib/mod.rs`：注册 `__method_make_generic`
- [x] 2.7 `MethodInfo.z42`：`IsGenericMethod`/`IsGenericMethodDefinition`/`GetGenericArguments()`/`MakeGenericMethod(params Std.Type[])` + 隐藏 `__typeArgs` 槽

## 阶段 3: 反射层级 + 构造函数反射
- [x] 3.1 `MethodBase.z42`（NEW）：`: MemberInfo`，共享 `Name`/`IsStatic`/`GetParameters()`/`__qualified`（数据成员，非抽象 Invoke）
- [x] 3.2 `MethodInfo.z42`：reparent `: MethodBase`，上移共享成员（保留 ReturnType/IsVirtual/泛型 API + `extern Invoke`→`__method_invoke`）
- [x] 3.3 `ConstructorInfo.z42`（NEW）：`: MethodBase`，`extern object Invoke(object[] args)`→`__ctor_invoke`
- [x] 3.4 `Type.z42`：`extern ConstructorInfo[] GetConstructors()`
- [x] 3.5 `reflection.rs`：`__type_constructors` native——按 ctor 命名约定（`<ClassFQ>.<ClassSimpleName>[$N]`）枚举，构造 ConstructorInfo（填 `__qualified`/params/IsStatic=false）
- [x] 3.6 `reflection.rs`：`__ctor_invoke(ci, args)` native——arity 校验（catchable）+ alloc 实例（同 `__activator_create` 分配 + typeArgs 具化）+ `invoke_qualified` 以新对象 reg0 跑 ctor + 返回对象
- [x] 3.7 `corelib/mod.rs`：注册 `__type_constructors` / `__ctor_invoke`

## 阶段 4: 测试
> 测试统一用 golden（`src/tests/`，走 `xtask test e2e`）——比 `[Test]` dogfood 更贴近端到端；
> 原计划的 `z42.core/tests/` [Test] 文件不再单列，其场景全由下面两个 golden 覆盖。
- [x] 4.1 IsGenericMethod / IsGenericMethodDefinition / GetGenericArguments（定义态占位 + 构造态实参）/ MakeGenericMethod 成功·arity 错·非泛型错 —— 由 `generic-method-invoke/reflect_generic_method.z42` 覆盖
- [x] 4.2 GetConstructors 枚举（带参/无参/多重载）；MethodBase 层级（ConstructorInfo 是 MethodBase）—— 由 `ctor-reflection/ctor_reflection.z42` 覆盖
- [x] 4.3 `src/tests/generic-method-invoke/`：静态 Invoke 返回值 golden
- [x] 4.4 同上：实例 Invoke（receiver + typeArgs 正交）
- [x] 4.5 同上：typeof(T) 反射==直接调用；default(T) 值/引用；new T() GetType
- [x] 4.6 同上：反射 throw 保留原异常类型（try/catch 捕获）
- [x] 4.7 `src/tests/ctor-reflection/`：带参 Invoke 建实例（字段初始化正确）；无参 ctor；arity 错；ctor 内 throw 保类型

## 阶段 5: 验证 + 文档
- [x] 5.1 `cargo build --release`（z42vm）无错
- [x] 5.2 `xtask test e2e` + `e2e --dir cross-zpkg` 全绿
- [x] 5.3 `xtask test stdlib` 全绿（REAL EXIT=0；z42.core 反射 dogfood 6+7 通过）
- [x] 5.4 `xtask test compiler` 自举 5/5 字节不动点（gen1==gen2）
- [x] 5.5 `xtask test vscode-syntax`（grammar in sync；lexer 未触及）
- [x] 5.6 spec scenarios 逐条覆盖确认（两个 capability）
- [x] 5.7 `docs/book/src/language/generic-methods.md` 加反射式调用节（数据流 + mermaid）
- [x] 5.8 反射式调用 + MethodBase/ConstructorInfo/构造函数反射机制并入 `generic-methods.md`（未单建 reflection.md——反射式调用是 M1 的反射对偶，同页承载最自然；已挂 SUMMARY）
- [x] 5.9 `docs/roadmap.md` G2 泛型方法 Invoke ✅ + 构造函数反射 + Deferred Backlog 加 3 条索引
- [x] 5.10 目录 README 同步（z42.core Reflection / z42.ir 若入口变化）

## 备注
- **无格式 bump**（实施期发现 SIGS 段已预留方法类型形参槽）→ 本地 warm build 用 0.41 nightly 种子正常跑，无跨版本自举墙、无 fixture、无两代自举。CI 走快路径。
- 现有 stdlib/z42c 源零泛型方法声明 → writer 改动不影响任何现有方法字节 → 自举字节不动点 gen1==gen2 不受影响。
- 别设 Z42_HOME=旧种子做 warm build（feature skew 假 E0401）。主树 z42-test 并发共享——本 change 隔离在 ../z42-genreflect（种子=下载的 0.41 nightly SDK）。
