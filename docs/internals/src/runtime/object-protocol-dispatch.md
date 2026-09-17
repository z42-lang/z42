# 对象协议的运行期派发（ToString / Equals / GetHashCode / GetType）

> 对齐：2026-09-17 ｜ 代码：`interp/dispatch.rs`（`obj_to_string`）、`interp/vcall_resolve.rs`
> （统一接收者阶梯）、`corelib/convert.rs`（`value_to_str`）、`corelib/object.rs`（`__obj_*` builtin）

`Std.Object` 的四个协议方法在**每一种 `Value` 变体**上都必须有答案——对象、裸基元、数组、装箱盒、
栈句柄。本页写这套「一个方法名 → N 种接收者表示」的派发是怎么落地的，以及为什么至今还留着
**两条**入口（`ToStr` 指令与 `VCall` 指令）而不是一条。

用户面的契约（哪四个方法、覆写 `Equals` 必须同时覆写 `GetHashCode`、struct 不继承 `Object`）见
reference 的[类](../../../reference/src/language/classes.md)一页，本页不复述。

## 两条入口

| IR 指令 | 发射处 | 走哪条 |
|---|---|---|
| `ToStr dst, src` | 字符串插值 `$"..."` 的每个洞（`ExprEmitter.z42:300`） | `dispatch.rs::obj_to_string`（**认 vtable**）|
| `Add dst, a, b`（操作数之一是 `Str`） | `+` 字符串拼接（`OperatorEmitter.z42:47-56`，**没有**发 `ToStr`）| `exec_value.rs::add` → `value_to_str`（**不认 vtable**）|
| `VCall dst, recv, method, args` | 每一处 `obj.ToString()` / `.Equals(..)` / `.GetHashCode()` / `.GetType()` | `vcall_resolve.rs::resolve_vcall` 阶梯 |

> ⚠️ **`+` 与插值不等价**，这是一个活着的语义裂缝。`exec_value.rs:58-59` 的 `Add` 混合臂直接调
> `value_to_str(vb)`，而 `value_to_str` 对 `Value::Object` 只会 `format!("{}{{...}}", 类型名)`——
> **用户写的 `override ToString()` 在 `+` 路径上被完全绕过**：
>
> ```
> class Point { ... override string ToString() { return "(1,2)"; } }
> $"a = {a}"       →  a = (1,2)        （ToStr → obj_to_string → vtable）
> "a is " + a      →  a is Point{...}  （Add → value_to_str，不查 vtable）
> ```
>
> 修法是让 `Add` 的混合臂走 `obj_to_string`（它已经有 `ctx`/`module`），或让 emitter 在
> 「`+` 且另一侧是 string」时先发 `ToStr`。两者都会改 golden 字节，未做。

## `ToStr`：`obj_to_string` 的两段

`dispatch.rs::obj_to_string(ctx, module, val)` 只分两支，不是三支：

**`Value::Object`** —— vtable 优先，builtin 兜底：

1. 从对象的 `type_desc` 取 `vtable_index["ToString"]` （O(1) 哈希）。
2. 命中 → `vtable[slot].1` 得函数限定名 → `module.func_index` → `exec_function` → 拆 `ExecOutcome`：
   - `Returned(Some(Value::Str(s)))` → `s`
   - `Returned(Some(other))` → 退回 `value_to_str(&other)`（防御；返回类型已是 `string`，不该发生）
   - `Returned(None)` → 空串（同上，防御）
   - `Thrown(v)` → 渲染成 `<exception: {value_to_str(v)}>`。**不重新抛** —— 从 IR 的视角
     `ToStr` 是不会失败的指令，把异常变成字符串是刻意的：插值洞里抛异常会让诊断路径自己炸掉。
3. 未命中 vtable（类没覆写 `ToString`，继承 `Std.Object` 的那个）→ 调 `__obj_to_str` builtin，
   返回不含命名空间的短类名。

**其余全部变体** → `value_to_str`。

## `value_to_str`：不可失败的 Display

`corelib/convert.rs:225`。它**没有 `ctx`**，这是它的形状的根本约束：凡是需要解 arena 或跑
z42 函数才能渲染的变体，它只能给占位符。

| `Value` 变体 | 输出 |
|---|---|
| `I64` / `F64` / `Bool` / `Char` | Rust `to_string()` |
| `Str` | 自身 |
| `Null` | `null` |
| `Array` | `[e0, e1, …]`（对元素递归）|
| `Object` | `类型名{...}`（**不查 vtable**，见上面的裂缝）|
| `FuncRef(n)` | `<fn n>` |
| `Closure(c)` | `<closure 被提升函数名>` |
| `BoxedStruct` | ① `boxed_enum_name()` 命中 → **enum 成员名**；② 否则 `boxed_prim_i64()` 命中 → 裸标量；③ 否则 `类型名{...}` |
| `PinnedView` / `StackClosure` / `Ref` / `StackObject` / `StackArray` / `StructRef` / `StructRefHeap` | `<pinned view>` / `<closure>` / `<ref>` / `<stack object>` / `<stack array>` / `<struct value>` —— 都是 arena 句柄，**无 `ctx` 可解** |

占位符臂不是死代码：栈句柄理论上到不了这里（`ToStr` 是逃逸汇点 ⇒ 被它读到的对象一定堆分配，
见 [逃逸分析](escape-analysis.md)），留着是为了逃逸分析误判时能在输出里一眼认出来，而不是
静默答错。

`BoxedStruct` 的三级判定顺序不能调换：enum 盒同时满足 `boxed_prim_i64()`（它的标量就是底层
i64），先查 enum 名才能让 `Console.WriteLine(Color.Blue)` 与 `((object)Color.Blue).ToString()`
说同一句话。两次查询共用一次 `borrow()`——entry 的 `Mutex` 不可重入，在第一个 guard 下再借会死锁。

## `VCall`：统一的接收者阶梯

`unify-vcall-resolution`（2026-09-03）之前，interp 与 JIT 各自抄了一份接收者阶梯、候选名生成和
PIC 安装规则（约 900 行手工保持同步，JIT 那份的 `resolve_virtual` 甚至是对 `module.functions`
的线性扫）。现在**「调谁」只在 `interp/vcall_resolve.rs::resolve_vcall` 决定一次**，两个引擎各自
只决定「怎么调」（interp 帧 vs 原生 `FnEntry`，以及各自的冷路径 / 跨 zpkg 兜底）。

阶梯顺序即语义，先命中者胜：

| # | 接收者 | 处理 |
|---|---|---|
| 1 | **基元盒**（`BoxedStruct` 且 `boxed_prim_i64()==Some`）| 无参 `GetType` 原生作答（盒保留精确 wrapper 类）；其余把 `this` 拆回标量，再解析 `{wrapper}.{m}` / `Std.Object.{m}` |
| 2 | **struct 盒** | `GetType` / `GetHashCode` / 非 record 的无参 `ToString` 原生拦截；否则解析 `{struct}.{m}` / `Std.Object.{m}`，`this` = 盒 |
| 3 | **裸基元 / 数组**（`primitive_class_name`）| 解析 `{Std.Int32\|Std.String\|Std.Array…}.{m}` / `Std.Object.{m}`，`this` = 值本身；全落空则落到第 4 级由它报「非对象接收者」 |
| 4 | **对象** | `vtable_index` → `dispatch::resolve_virtual`（模块内类链）→ 经 `ctx.try_lookup_type` 的惰性基类链走（跨 zpkg 基类）|

解析结果是 `VCallTarget` 四态：`Immediate(Value)`（第 1/2 级的原生拦截，一个 z42 函数都没跑）、
`Local(idx)`、`Lazy(Arc<Function>)`、`Thrown(Value)`（`fix-call-arity-skew`：按站点键找到了定义
但其签名吃不下这次调用的实参，携带 `MissingSymbolException`，**绝不进 PIC**）。

第 4 级里 `vtable_index` 必须排在 `resolve_virtual` **前面**（`fix-jit-vcall-overload-dispatch`）：
vtable 把「可能重载的方法名」映射到编译器为该站点绑定的那个覆写槽，而 `resolve_virtual` 的
`Class.method` 字符串走法会撞上先遇到的同名函数。

### overload-suffix 重试

候选名由 `resolve_by_candidates`（`vcall_resolve.rs:309-342`）生成，每个类压三种拼法：
`{c}.{m}${arity}`、`{c}.{m}`，以及当操作数本身已是完整键（`Name$arity$types`）时的裸规范槽；
然后对 `Std.Object` 再压同样三种。

`arity_first` 参数控制前两者的先后——基元路径（第 3 级）历史上是「先裸名后 mangle」，盒路径
（第 1/2 级）是反的，这个不对称是保字节不动点留下的，不是语义需要。

为什么必须重试：**IR 里的方法名未必带 mangle**。接收者静态类型是 `object` 时（`Std.Assert.Equal(object, object)`
里的 `expected.Equals(actual)`）IR 带的是裸名 `Equals`，而编译器在**声明处**总是给重载加后缀
（`Std.String` 上 `Equals(object?)` 与 `Equals(string)` 两份 ⇒ 注册名带 `$1$…`）。两边拼法对不上，
只能在派发端把两种都试一遍。

### PIC 安装

凡是解析到的 callee 属于本模块、且接收者有类型 id（对象用真 `TypeDesc.id`，基元用合成
`PRIM_TYPE_*`；**盒没有** id），就把 `(type_id, slot, fn_idx)` 写进站点的 `VCallIC`，下一次同类型
接收者走 `vcall_ic_hit`、根本进不到本模块。安装前还要校验目标签名吃得下这次的实参
（`install_ic`，`fix-call-arity-skew`）——缓存一个 arity 不匹配的目标，等于让之后**每一次**调用
都从 PIC 直接派发到错的函数、绕过 `resolve_vcall` 里的检查。

细节见 [inline cache 发布](inline-cache-publication.md)与 [interp / JIT 语义一致性](interp-jit-semantics.md)。

## `Equals` / `GetHashCode` / `GetType` 没有第二套机制

它们全部走上面那条 `VCall` 阶梯，**没有**各自的 `dispatch.rs` helper。三件事值得单记：

- **基元的 `Equals` / `GetHashCode` 已经没有 builtin 了**。`__int32_equals` / `__int32_hash_code` /
  `__double_equals` / `__double_hash_code` / `__char_equals` / `__char_hash_code` 在
  `shrink-primitive-native-interop` Stage 2（2026-08-28）删除（`corelib/builtin_table.rs:88-89`），
  现在是 `Std.Int32` 等 wrapper 里的纯脚本方法（`Equals` → `this == other`，`GetHashCode` → `(int)this`）。
  `Std.String.ToString` 同理，2026-08-27 起是 `String.z42:65` 的 `return this;`——旧的
  `__str_to_string` builtin 只是原样返回自身，不值得占一个 `BuiltinId`。
- **`GetType`** 在阶梯的第 1/2 级是原生拦截（盒自己带精确 type_desc），在第 3/4 级经
  `Std.Object.GetType` → `__obj_get_type` builtin（`corelib/object.rs`）合成 `Std.Type` 对象。
  `Std.Type` 自己的 `GetType()` 有一个独立的 vtable 撞车坑，见
  [反射 Type 身份](reflection-type-identity.md)。
- **装箱值的四方法**由「基元盒 / struct 盒」分流决定，判据是 `ScriptObject::boxed_prim_i64()` 的
  `Some`/`None`。那张逐消费点的分流表在 [struct 值语义](struct-value-semantics.md)，本页不复述。

## 为什么是两条入口而不是一条

`ToStr` 与 `VCall` 分开，不是历史遗留，而是两种不同的契约：

1. **`ToStr` 必须不可失败**。它长在插值和拼接这类「本身就在渲染诊断信息」的位置，抛出去就会
   让报错路径自己炸。所以它吞异常、对未知表示给占位符、对没覆写 `ToString` 的类走 builtin
   短名——**永远有一个字符串**。
2. **`VCall` 必须忠实**。`obj.ToString()` 是一次普通的虚调用，异常要照常传播，重载要照常按
   arity / 签名解析，PIC 要照常安装。
3. 两者的接收者面也不同：`ToStr` 只需要覆盖「能出现在插值洞里的值」，`VCall` 要覆盖全部七种
   可调用接收者形态（含栈句柄与两类盒）。

真正该收敛的不是这两条入口，而是上面那条 `+` 的裂缝——它现在走的是 `value_to_str`，既不是
`ToStr` 的语义也不是 `VCall` 的语义。

## 关联文档

- [VM 架构](vm-architecture.md) —— `Value` 变体全表与解释器主循环
- [struct 值语义](struct-value-semantics.md) —— 装箱模型（基元盒 / struct 盒）的 SoT
- [反射 Type 身份](reflection-type-identity.md) —— `Type` 对象自身的 `GetType`、数组类型名
- [interp / JIT 语义一致性](interp-jit-semantics.md) —— 同一语义两处实现的总纲
- [inline cache 发布](inline-cache-publication.md) —— PIC 安装与失效
