# 解释器 / JIT 标量语义的单一真相源

> 代码：`src/runtime/src/semantics.rs`（标量真相源）、`src/runtime/src/objops/`（对象操作真相源）、`src/runtime/src/interp/ops.rs` +
> `interp/exec_value.rs`（interp 消费）、`src/runtime/src/jit/helpers/arith.rs` +
> `jit/helpers/object.rs`（JIT helper 消费）、`src/runtime/src/jit/translate/emit_int.rs` +
> `emit_fc.rs`（JIT 内联镜像）。

## 问题：同一语义三处实现

z42 的算术 / 比较 / 数值转换语义，运行时有**三条执行路径**各自表述同一套标量规则：

1. **interp 执行循环** —— 按 `Value` 类型 match，走寄存器级包装（`ops.rs`）。
2. **JIT runtime helper** —— `extern "C"` 函数，再做一遍类型判断。
3. **JIT 内联 Cranelift** —— 当 `reg_types` 静态证明操作数同型（全 I64 / 全 F64）时，
   JIT **绕过 helper**，直接发 Cranelift 原语（`iadd`/`fcmp`/`fcvt_to_sint_sat`…）。

改一处语义（wrapping 策略、除零行为、新数值类型）需记得同步三处；漏改即 interp 与 JIT
行为分歧。对「编译器全自举 byte-identical」目标，这是致命隐患（整数除零、char 比较都出现过漂移）。

## 机制：能共享的共享，不能的锚定 + 差分

### 路径 1 + 2：真共享（运行期同一 Rust 函数）

`semantics.rs`（crate 顶层，interp / jit / 未来 AOT **对称依赖**）持**值级标量规则**：

```
int_binop(va, vb, int_op, float_op)   numeric_lt(va, vb)   eval_cmp(op, va, vb)
int_bitop(va, vb, op)                 convert_value(v, to_tag)
is_int_div_by_zero(divisor)  DIV_BY_ZERO_EXC  div_by_zero_msg(op)  SHIFT_MASK
```

- interp `ops.rs` 保留**寄存器取值**（「undefined register」错误是 interp 执行模型专属），
  取到 `&Value` 后调 `semantics::*`。
- JIT helper（`arith.rs`）不自带 `int_binop_helper`/`numeric_lt_helper` 之类副本，统一调 `semantics::*`；`jit_convert` 委托 `semantics::convert_value`。

于是路径 1、2 对同一规则只有**一份**实现。

> ⚠️ **相等比较必须同样走 `semantics::*`。**
> `Value: PartialEq` 按变体配对、**没有混合数值臂**；若 `jit_eq` / `jit_ne`（或 interp 侧
> `eval_cmp` 的 `Eq`/`Ne`）直接用它，`int == double` / `char == int` 会在**三路全部**恒假，
> 而同样操作数的 `<` `<=` `>` `>=`（走 `semantics::numeric_lt`）却正确。
>
> 两条要点：
>
> 1. **差分测试对混合操作数是盲区。** 它比的是内联码与 `semantics.rs` 的 byte-identity，
>    而混合操作数**根本不产生内联码**（`is_int_cmp` / `is_f64_cmp` 只在两侧静态同类时内联）。
>    混合路径只能靠 golden（`src/tests/operators/mixed_numeric_equality.z42`）+ 单测守。
> 2. **新增 / 改动任何比较 helper 前，先确认它调的是 `semantics::*`**，而不是自己就地写一个
>    看起来等价的 `==`。「等价」在同类操作数上成立、在混合操作数上不成立，正是漏网的形状。

**对象级判定同款**：标量之外，两条对象路径也已收敛到运行期同一 Rust 函数——

| 判定 | 单一实现 | 缓存 | interp 调用侧 | JIT 调用侧 |
|------|---------|------|--------------|-----------|
| 虚调用目标（`VCall`） | `interp/vcall_resolve.rs::resolve_vcall` | 站点 PIC（`VCallIC`） | `exec_vcall.rs` | `helpers/vcall.rs` |
| 类型判定（`is` / `as` / 带类型 `catch`） | `interp/dispatch.rs::isa_td` | `vm_context/isa_cache.rs`（id 键直接映射）→ `subclass_memo`（同键 memo）→ 基链/接口遍历 | `exec_object.rs`、`find_handler` | `helpers/object.rs`、`helpers/control.rs` |

#### 类型判定的缓存键

`IsaCache` 与 `subclass_memo` 的键都是 `(接收者 TypeId, 目标键)` 这一对 u32，**不用任何地址**：

- **接收者**：`TypeDesc.id`。进程级发号、永不复用（见 [vm-architecture.md](vm-architecture.md)「TypeId 的作用域」），
  描述符被释放、地址被别的类型复用（可回收的 load context、REPL 轮次）也不会让键误命中。同一类型的几个描述符版本
  （主模块里只含自身字段的那份、加载器 fixup 出的合并副本）同 id、同名、同基类链，判定相同。没有 id 的描述符
  （`UNRESOLVED`：按对象现建的回落描述符、corelib 原生句柄单例）照常走遍历作答，**不进缓存**。
- **目标键**：站点上的 `TypeKeyCell`（`IsInstanceInsn.target` / `AsCastInsn.target` / `ExceptionEntry.catch_key`；
  `is_exception_subclass` 用一个 `static`）。首次判定时由 `dispatch::target_key` 按名解析一次：名字是已登记类型
  （主模块注册表，再查惰性加载器——只查不加载）→ 取它的 `TypeId`；否则（擦除名 `Demo.GBox`、arity 形 `GBox$1`、
  所在包尚未加载的接口）→ `TypeTable::name_key` 为这个**名字**保留一个 id（同一发号器，不会分给描述符）。
  两种键都进程唯一、各自只代表一个目标名，而判定只取决于「接收者类型的名字链 + 目标名」，所以键可以放在
  共享元数据里，不论哪个 VM 解析的都对；目标类型后来才加载、键仍是先前保留的那个，判定照旧正确。
  反射等只有名字的调用方（`is_subclass_or_eq_td`）每次按名求键，走同一份 memo。

`IsaCache` 每槽是**一个** `AtomicU64`：`recv << 32 | verdict << 31 | target`（两个 id 都在 `IMPORT_BASE = 1<<31`
以下，第 31 位空出来放判定），一次 store 安装、一次 load 读出，三者不可能撕裂；空槽是 `u64::MAX`（接收者
`UNRESOLVED`，永不匹配）。命中 = 一次 relaxed load + 一次比较，无锁无哈希；直接映射、冲突覆盖，未命中回 memo
（`Mutex<FxHashMap<u64, bool>>`，键同一个打包值）再回填。两者在显式模块 (re)load（REPL 重定义）时一起清空。
JIT 侧不自带子类遍历：`jit_is_instance` / `jit_as_cast` / `jit_match_catch_type` 多收一个烘焙的 `TypeKeyCell`
指针（指令 / 异常表行里的那个，与代码同寿），同样调 `isa_td`。

#### 缓存键必须是**全局**类型身份

`IsaCache` 与两条 PIC（`VCallIC` / `FieldIC`）都以 `TypeDesc.id` 这个裸 `u32` 做键（命中 = relaxed load + 比较）。

于是它们的正确性完全押在**「`TypeId` 在比较发生的范围内唯一」**上。若 `TypeId` 每个 `Module` 从 0 重开（per module），跨 zpkg 的 `TypeDesc`
由惰性加载器**原样返回**、保留外来模块的号，任何**跨 zpkg 多态**的站点就会把后到的
receiver 误命中先到者的条目：

```
site: body.Run(i)            // IParallelBody 接口调用，z42c.semantics
  第一次: receiver = SrcReadHashTask (z42c.driver,   TypeId 139) → 装 PIC
  第二次: receiver = CompileCuTask   (z42c.emission, TypeId 139) → 误命中
      ⇒ 跑 SrcReadHashTask.Run，其 this._srcs[i] 读到 CompileCuTask 槽 0 的 _cus[i]
      ⇒ File.ReadAllText(<CompilationUnit>)，自举链当场崩
```

`FieldIC` 撞键更隐蔽：拿到**错误的字段槽**，不崩不报错，直接静默读写错数据。

**不变量**：`TypeId` 由进程级发号器 `tokens::alloc_type_id_block` 批量分配、**全进程
唯一**，号段限定 `[0, IMPORT_BASE)`，越界 panic（回绕等于引入撞号）。debug 构建在两条
PIC 的命中点各设一道常驻断言（`vcall_resolve::assert_pic_target` 校验 callee 归属；
`resolver::assert_field_ic_slot` 校验槽位），违反即**在误派发当场** panic；release 编译掉，
热路径不变。

> **可迁移的判据**：任何「在作用域 S 内发号、却拿到 S 之外做相等比较」的 id 都是这个形状的
> bug。要么把发号范围提升到比较范围（本系统的选择），要么改用天然全局的身份。指针看似天然全局，
> 但对象释放后地址会被复用，只在「元数据永不释放」的前提下成立——可回收 load context 正好打破它。

### 对象操作：objops

字段、数组、静态字段、值 struct 叶子的**读写语义**收在 `src/runtime/src/objops/`，两个引擎都只做适配：

```
interp exec_*            ┐                       ┌ Ok(v)  → 写 dst 寄存器
                         ├─ objops::<op>(&Value) ┤
JIT helpers/* (extern C) ┘                       └ Err(OpError) → 引擎的异常通道
```

| 指令 | objops 入口 | interp 适配 | JIT 适配 |
|------|------------|------------|---------|
| `FieldGet` / `FieldSet` | `field::field_get` / `field_set` | `exec_object.rs` | `helpers/object_field.rs` |
| `ArrayGet` / `ArraySet` / `ArrayLen` / `ArrayNew` / `ArrayNewLit` | `array::*` | `exec_array.rs` | `helpers/array.rs` |
| `StaticGet` / `StaticSet` | `statics::static_get` / `static_set` | `exec_object.rs` | `helpers/object.rs` |
| `StructFieldGetPrim` / `SetPrim` | `struct_leaf::struct_field_{get,set}_val` | `exec_struct.rs` | `helpers/struct_ops.rs` |
| `LoadElemAddr` / `LoadFieldAddr` 与经 `ref` 读写 | `array::check_elem_addr` / `elem_{load,store}`、`field::check_field_addr` / `{load,store}_named` | `exec_address.rs`、`frame.rs` | （JIT 不翻译，见 `unsupported_reason`） |

**错误通道只有一个**：`objops::OpError`。

- `Throw { class, msg }`：用户可 `catch` 的异常。类与消息**只在 `objops/error.rs` 里写**：

  | 情形 | 异常类 | 消息 |
  |------|-------|------|
  | 字段读 / 写的接收者为 null | `Std.NullReferenceException` | ``cannot read field `N` of a null reference`` / ``cannot write field …`` |
  | 数组读 / 写 / 取长的数组为 null | `Std.NullReferenceException` | `cannot read an element of a null array` 等 |
  | 下标越界（含负数） | `Std.IndexOutOfRangeException` | `index 3 is out of range for an array of length 3` |
  | 数组长度为负 | `Std.OverflowException` | `array size cannot be negative (got -2)` |
  | 基元字段写 null / 错类型 | `NullReferenceException` / `InvalidCastException` | ``cannot store null into primitive field `N` `` |
  | 严格 OOM 下分配失败 | `Std.OutOfMemoryException` | `cannot allocate array[n]: heap limit exceeded` |

  null 检查先于下标检查（`null[-1]` 抛 NRE）。`.Length` 经 `FieldGet` 到达，null 接收者报的是字段读。
- `Thrown(Value)`：已经构造好的异常（类型初始化失败、缺符号）。
- `Internal(anyhow)`：编译器发错码、栈句柄失效之类的 VM 内部错误，文本两侧同样相同。

`OpError::into_exception(ctx, module)` 把它物化成异常值，两个引擎共用：interp 的 `ops::raise` 返回
`Ok(Some(exc))`（进 `find_handler`）或 `Err`；JIT 的 `helpers::raise` 把它塞进 pending 槽并返回 1，内部错误
退化为字符串异常（helper 没有别的出口）。stdlib 异常类没加载时（裸模块的 Rust 单测）两侧都得到同一条
`<类名>: <消息>` 文本。

**快路不自带语义**。JIT 的循环不变量提升（`jit_obj_field_slot` / `jit_obj_ref_field_slot` /
`jit_array_data_opt`）只向 objops 要「存储地址」（`field::inline_prim_slot` / `inline_ref_slot`、
`array::packed_data`），**从不抛**：拿不到快路（null、非打包数组、栈数组、引用字段……）就回落到慢路 helper，
异常在真实访问点由 objops 给出。逐访问取打包数组数据也用同一个不抛的 helper。interp 的 FieldIC 命中路径
同样在 objops 里（`field::slot_of`），两引擎共用同一个站点 IC。

> **改存储表示只改 objops**。对象模型改造（`docs/runtime-audit.md` §7「对象模型（M10 + M11）已定方案」，
> R1–R9：原子单元格、8 B 自描述引用、数组模式、去掉每对象 Mutex……）以本层为前提：引擎里不得再出现
> `borrow()` 字段槽、`get_boxed` / `set_boxed`、`field_value` 之类的直接存储访问；新增对象 / 数组操作时先在
> objops 写实现，再给两个引擎各加一个适配。
>
> `Std.Array` 的无类型 / 批量原生（`CopyRange`、`GetValue`、`SetValue`）经 `objops::array_bulk`，corelib 只解析参数、
> 用 `corelib::raise_op` 把 `OpError` 以原异常类抛出。
>
> 尚未进入本层的：对象分配（`interp/obj_new_resolve.rs`）、闭包环境数组、反射 builtin
> （`FieldInfo.GetValue` / `SetValue` 等），以及 `VCall` 的 null 接收者（仍是
> `VCall: expected object, got Null` 内部错误）。

端到端对照：`src/tests/exceptions/objops_errors.z42` 在 interp 与 `--mode jit` 下各跑一遍，逐条断言异常类与
`Message`；每个出错的访问放在独立函数里，保证 JIT 档的异常确实出自 JIT helper。

### 路径 3：注释锚定 + 差分测试（无法运行期调 Rust）

内联路径发的是机器码，不能在运行期 `call` 一个 Rust 标量函数（那正是它要绕开的开销）。
故它以 **`// SEMANTICS: semantics::<fn>` 锚注释**引用对应规则，并由**边界 golden 差分测试**
（`src/tests/operators/arith_semantics_edge.z42`，在 interp 与 `--mode jit` 下各跑一遍、
断言 byte-identical）把「注释担保」升级为「测试担保」。

`semantics.rs` 模块文档以一张表钉住**七个易漂移的载重决策**，三路共同引用：

| 决策 | 规则 | 内联镜像 |
|------|------|---------|
| 整数 add/sub/mul 溢出 | wrapping | `emit_i64_binop`：`iadd`/`isub`/`imul` |
| 整数 div/rem 除零 | 抛 `Std.DivideByZeroException` | `emit_int_divrem`：冷路由 `b∈{0,-1}` |
| 整数 `MIN/-1`、`MIN%-1` | wrapping：得 `MIN`、`0`（`int_div` / `int_rem`） | `emit_int_divrem`：`b=-1` 冷路由到 helper |
| float→int | 饱和 + NaN→0；`U64` 按 signed i64 饱和 | `emit_f64_to_int`：`fcvt_to_sint_sat` |
| int→float | 全 f64 精度（F32 目标也走 f64） | `emit_int_to_f64`：`fcvt_from_sint` |
| 数值比较 | signed ordered；`Ne` unordered（`NaN!=NaN→true`） | `emit_i64_cmp`/`emit_f64_cmp` |
| 整数移位量 | mask 到低 6 位（`& SHIFT_MASK`） | `emit_i64_binop`：`Shl`/`Shr` 前 `band 63` |

> **为何 `MIN/-1` 守卫只在内联路径**：native `idiv` 在 x86-64 对 `i64::MIN / -1` 会 SIGFPE-trap，
> 故内联用 `(b as u64).wrapping_add(1) <= 1` 判 `b∈{0,-1}` 冷路由到 helper；helper 与 interp
> 都调 `semantics::int_div` / `int_rem`（`wrapping_div` / `wrapping_rem`），得 `MIN` 与 `0`。
> Rust 的裸 `x / y` 在这里会 panic（debug 与 release 都会），所以三路都不能写裸运算符。
> 边界 golden `arith_semantics_edge` 钉住 `MIN/-1`、`MIN%-1`，`exceptions/int_divide_by_zero` 钉住 `/0`、`%0`。

## 配套：JIT 不支持指令单表

「哪些 opcode JIT 不能翻译、原因是什么」在两处检查点使用（prescan `jit_unsupported_reason`
与 translate 的 `bail!` 臂）。由单个 `unsupported_reason(&Instruction) -> Option<&str>`
（`jit/translate/unsupported.rs`）：prescan 循环调它；每个 `bail!` 臂的原因文案也源自它，两检查点
不会漂移。须收 `&Instruction`（非静态 opcode 集）——generic `Call`/`VCall` 是**条件**不支持。

## 相关

- JIT 内联快路径与 helper 边界：[JIT 惰性逐函数编译](jit.md)、
  `src/runtime/src/jit/README.md`。
