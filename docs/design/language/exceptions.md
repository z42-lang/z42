# z42 异常机制

> **状态**：L1 throw/catch 语法 ✅；Wave 2（2026-04-25）补齐 stdlib `Exception` 类层次。
> **使用者视角**；实现原理见 `docs/design/runtime/vm-architecture.md`。

---

## 抛出与捕获

```z42
try {
    if (x < 0)
        throw new ArgumentException("x must be non-negative");
    DoWork(x);
} catch (Exception e) {
    Console.WriteLine(e.Message);
}
```

- `throw <expr>` 把任意值（Phase 1 兼容）或 `Exception` 子类实例传给最近的
  匹配 `catch` 块
- `catch (Exception e)` 当前为"通用 catch"，绑定抛出的值（无类型过滤）
- 未来 L2/L3 可能加入按类型过滤（`catch (ArgumentException e) { ... }`）

## Exception 基类

```z42
public class Exception {
    public string Message;          // 异常消息
    public string StackTrace;       // 调用栈快照（Phase 1 占位，恒为 null）
    public Exception InnerException; // 包裹的原始异常（无则 null）

    public Exception(string message) { ... }

    override string ToString();  // "Exception: <Message>"
}
```

**Phase 1 限制**（Wave 2 实施时记录的 backlog）：

1. **字段不支持 `?` 可空标注** — Parser 限制；当前 ref-type 字段默认
   nullable（运行时正确，但类型层面无 explicit null tracking）
2. **构造器不支持重载** — VM ObjNew 按 `Class.SimpleName` 查找 ctor，
   stdlib 编译生成 `$1`/`$2` arity suffix 时无法命中。当前 Exception
   仅一个 ctor；wrapping pattern 需用 setter（见下）
3. **同类型字段 self-reference assign 报 E0402** — TypeChecker 限制；
   用户代码 `outer.InnerException = inner;` 当前无法编译。Wave 2 不
   尝试在用户代码层使用 InnerException；stdlib 内部抛出可以维持空 inner

## 标准子类

| 子类 | 继承自 | 何时用 |
|------|--------|--------|
| `ArgumentException` | Exception | 参数非法（值 / 组合不符合契约）|
| `ArgumentNullException` | ArgumentException | 参数为 null（更细分）|
| `InvalidOperationException` | Exception | 对象状态不允许（如空 Queue Dequeue）|
| `NullReferenceException` | Exception | 解引用 null |
| `IndexOutOfRangeException` | Exception | 索引越界 |
| `KeyNotFoundException` | Exception | 字典 / Map 找不到键 |
| `FormatException` | Exception | 字符串解析失败（`int.Parse` 等）|
| `NotImplementedException` | Exception | 方法未实现 |
| `NotSupportedException` | Exception | 方法不支持当前场景 |

每个子类只有 ctor 转发 + override ToString 返回 `"<ClassName>: <Message>"`
（硬编码类名字符串，不依赖反射；L3-R 后可重构为统一 `GetType().Name`）。

## Phase 1 兼容性

- 仍支持 `throw "string"` / `throw 42` 等任意值抛出（保持 L1 行为）
- stdlib 自身 / 新代码**推荐**用 `throw new <ExceptionType>(...)`
- 非 Object 抛出（string / int 等）只能被 **bare `catch { }`** 接住；不再被 typed `catch (Exception e)` 捕获（catch-by-generic-type，2026-05-06）

## catch 按类型过滤（2026-05-06）

由 [`docs/spec/archive/2026-05-06-catch-by-generic-type/`](../../spec/archive/2026-05-06-catch-by-generic-type/) 落地。修复"所有 catch 子句一律 wildcard"的 Phase 1 语义 bug — 之前 `catch (NullReferenceException e)` 也会捕获 IOException，类型断言形同虚设。

### 语义

| 形式 | catch_type IR 字段 | 匹配规则 |
|------|-------------------|---------|
| `catch (T e)` / `catch (T)` | `Some("FQ.T")` | thrown.class instance-of T（沿 BaseClassName 链上溯）|
| `catch { }` | `None` | 任意 thrown — 包含非 Object 的 Phase 1 legacy 抛出 |
| 编译器合成的 finally fallthrough | `Some("*")` | 任意 thrown（synthetic catchall）|

多个 catch 子句按**源码顺序**第一个匹配者胜（C# / Java / Python 标准）。VM 端 `find_handler` 沿源序扫 exception_table，每条按上表 None / `"*"` / typed 三分支判定，首个匹配即返回。

### 编译期校验（E0420 InvalidCatchType）

`catch (T e)` 在 TypeChecker 阶段必须满足：
1. `T` 是已加载（本 CU 或 imported）的 class 名
2. 沿 `BaseClassName` 链可上溯到 `Std.Exception`（`Exception` 自身亦视为合法）

否则报 `E0420 InvalidCatchType`：

```z42
catch (NoSuchType e) { }     // E0420: catch type 'NoSuchType' not found
class Foo { } catch (Foo e)  // E0420: catch type 'Foo' must derive from Exception
```

E0420 不阻塞编译（compiler emit `null` catch_type → 退化为 wildcard），让用户在同一编译批次看到所有诊断。

### 实现路径

- **AST**：`CatchClause.ExceptionType: string?` 早已捕获用户写的类型名（pre-existing）
- **TypeChecker**：[`TypeChecker.Stmts.cs::TryResolveCatchType`](../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Stmts.cs) 解析 short → FQ + 校验 Exception 派生，写入 `BoundCatchClause.ExceptionTypeName`
- **IrGen**：[`FunctionEmitterStmts.cs`](../../src/compiler/z42.Semantics/Codegen/FunctionEmitterStmts.cs) 直接把 BoundCatchClause.ExceptionTypeName 给 `IrExceptionEntry.CatchType`（IR / zbc 字段早已存在，无格式 bump）
- **VM interp**：[`interp/mod.rs::find_handler`](../../src/runtime/src/interp/mod.rs) 接 `&Module.type_registry` + `&Value`；每个 entry 的 catch_type 走 None / `"*"` / typed-via-`is_subclass_or_eq_td` 三分支
- **VM JIT**：新 helper `jit_match_catch_type(target_ptr, len) -> i8` 在 ctx.exception 上 peek + 类型链匹配；JIT throw 终结子按 catch_chain 顺序生成 `match_catch_type → brif → install_catch + jump` 链；wildcard 单 entry 走 fast-path

### 泛型 catch（透明 piggyback）

`catch (MulticastException<int> e)` 也透明工作：编译器把 type-args 编入 mangled FQ 名（z42 generic instantiation 早已使用），VM 端字符串比较 + 链式上溯即足够，无需类型反射或 monomorphize 期改动。但 D-8b-0（class registry arity-aware）落地前，同名 generic / non-generic 共存仍被早期阶段挡住。

## Stack-trace 符号化与 sidecar (`.zsym`)（2026-05-10 split-debug-symbols）

z42 通过 zsym sidecar 实现 release 构建的"小体积 + 离线可符号化"双目标。

### 形态

- **debug 构建**（默认）：`.zpkg` 内嵌 DBUG section（含 LineTable + LocalVarTable）；trace 直接显示 `at <FQN>(<sig>) (<file>:<line>:<col>)`
- **release 构建**（`[profile.release].strip = true` 或 `--strip-symbols=true`）：
  - `<name>.zpkg`：剥离 DBUG bodies + 写 16 字节 BLID section（MurmurHash3 x86_128）
  - `<name>.zsym`：sidecar zpkg（`ZpkgFlags.SymOnly`）+ 单独 STRS（debug 字符串子集）+ MDBG（per-module DBUG）+ BLID
  - 两者通过 BLID 字节配对（runtime 加载时校验，离线工具同样依赖）

### Runtime 自动加载

runtime 加载 `<path>/<name>.zpkg`（或 `.zbc`）后，自动探测同目录 `<name>.zsym`：

```
load_zpkg(path):
  1. parse main zpkg (FUNC bodies, SIGS, ...)
  2. probe `<path-stem>.zsym`
     ├─ exists & SymOnly flag & BLID matches → merge DBUG into per-module funcs
     ├─ exists but BLID mismatch / corrupt    → log warn + ignore (load 不失败)
     └─ absent                                 → 静默退化（trace 走 fallback）
  3. continue with normal load pipeline
```

加载到 sidecar 后 trace 与 debug 构建无差异。无 sidecar 时 trace 退化形式（前缀 `at <FQN>(<sig>)`，但无 `(file:line:col)` 段）。

### 函数签名 (1.3+)

trace 中每帧函数名携带参数类型签名：

```
at MyApp.Greeter.greet(Greeter,str) (Greeter.z42:14:5)
```

签名来源：SIGS section 中每函数 `paramCount × u32 strIdx` 编码（zbc 1.3 / zpkg 0.4）。实例方法包含隐式 `this`（类型 = 接收者类裸名）作为 index-0 entry。SIGS 无对应名称时填 `?` 占位（旧产物 / synthesized 函数）。

### 离线符号化（独立工具，spec follow-up）

`z42c symbolicate <crash.txt> --syms <name>.zsym` 把 trace 中 `+0x<ip> [build:<8hex>]` 形式的 frame 还原为 `(file:line:col)`。当前 spec 内未实施；见 [docs/spec/changes/split-debug-symbols/tasks.md](../../spec/changes/split-debug-symbols/tasks.md) 阶段 2.3。

### Deferred / Future Work

- **`+0x<ip> [build:<8hex>]` 退化格式**：当前实现 line==0 时帧只显示函数名 + 签名，未携带 ip / build_id。要支持完整离线符号化需 VmFrame 追踪 current PC 并在 snapshot 时携 module BLID。留 follow-up。
- **`z42c symbolicate` CLI 工具**：见上。
- **Lazy / mmap sidecar**：当前 eager 加载，启动 IO 一次性付清；启动延迟敏感场景可加 lazy 路径。
- **跨目录 sidecar 搜索**：仅探测同目录；debuginfod 风格（环境变量 / URL）独立 spec。
- **stdlib 公开 `Std.Reflection.Symbolicate`**：让 z42 程序内部触发符号化。

## 未来计划（按需推进）

| 项 | 说明 |
|---|----|
| StackTrace 自动填充 | 需 VM 在 throw 时收集帧链；零 source 改动可上线 |
| Exception ctor 重载 | 等 VM ObjNew 支持 arity suffix 查找后补 `Exception(msg, inner)` |
| 字段 `?` 可空标注 | 等 Parser / TypeChecker 扩展字段 nullable |
| 同类型字段 self-assign | E0402 修复；解锁用户代码 InnerException 链使用 |
| 派生子类扩展 | `OverflowException` / `DivideByZeroException` / `IOException` 按需加 |
| `catch (e)` 无类型 + var 语法 | 当前 parser 把单 ident 当 type；要新语法（如 `catch (var e)`）才能"untyped with var"|
| Exception filter `catch (T e) when (cond)` | 待评估（C# 风格条件 catch；与 D-8b-2 不冲突）|
