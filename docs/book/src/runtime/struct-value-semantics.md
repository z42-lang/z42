# struct 值语义（内联字节 blob）

> 状态：A-use 落地（2026-08-09，zbc 1.31 / zpkg 0.36）；嵌套字段 + `struct==` 值相等（2026-08-10）+
> struct→object 装箱身份 + 合成对象协议方法 + 泛型容器装箱（2026-08-11）落地，均无格式 bump。本页讲**多字段 struct 的真值语义**
> 如何在编译器 + 运行时实现。程序全景（选项 B / B-radical 统一值类型 / 分阶段）见
> `docs/spec/changes/add-struct-value-semantics/`。

## 目标

z42 的 `struct` 是 **C# 真值类型**：赋值 / 传参 / 存容器 = **字段级复制**，不是共享堆对象引用。

```z42
struct Point { public int x; public int y; public Point(int x,int y){this.x=x;this.y=y;} }
var a = new Point(1, 2);
var b = a;      // 值复制
b.x = 99;       // 只改 b
// a 仍是 (1,2)  —— 引用语义下 a.x 会跟着变成 99
```

现状（A-use 前）：`struct == class == Value::Object(GcRef)`，`b=a` 克隆句柄 → 串味。A-use 把
**多字段复合 struct** 翻转为内联字节 blob 值语义。

## 布局：字节精确

编译器 `StructLayout`（`z42c.semantics`）为每个 struct 类型算**字节扁平布局**：每个直接字段的
`(byte_offset, size, kind)`、类型总 `size/align`，以及**带种类的引用叶子表**（引用位图）。

- 基元字节精确：`i8/u8/bool=1`、`i16/u16=2`、`i32/u32/f32/char=4`、`i64/u64/f64=8`（`char`=4B
  Unicode 标量）。
- 引用叶子（`string` / object / array 字段）= 16B 托管句柄，进**引用位图**（种类 ArcString / GcRef）。
- `Point{x:int,y:int}` → `size=8, {x:@0, y:@4}`，引用位图空。

## 运行时：per-context 字节 arena + 侧表引用叶子

未装箱 struct 值 = per-`VmContext` **字节 arena**（`interp/struct_arena.rs`）里的一段 blob；寄存器持
`Value::StructRef{idx, frame_id}` 句柄（仿 `StackObject`，LIFO 随帧退出截断，`frame_id` staleness 守卫）。

一个 blob（`StructSlot`）= **两部分**：

| 部分 | 存什么 | 为什么分开 |
|------|--------|-----------|
| `bytes: Box<[u8]>` | 基元叶子，**字节打包**在布局偏移处 | γ 密度：逼近 C# 内存密度 |
| `refs: Box<[Value]>` | 引用叶子（`string`/object/array），作**真 `Value`** | Rust 内存安全：`Arc<str>`/`GcRef` 的裸字节写进 `[u8]` 会漏引用计数（泄漏/double-free）、moving GC 也无法改写；侧表由 `Value` 的 clone/drop 正确托管 |

`refs` 按引用位图（`ref_offsets`）排序；字段访问按 byte offset 映射到 `refs` 槽。

### 四条 IR 指令（zbc opcode 0xC0–0xC3）

| 指令 | 语义 |
|------|------|
| `StructAlloc dst, type_name, size` | 在 arena 分配零初始化 blob，`dst` = StructRef 句柄 |
| `StructCopy dst, src, size` | 复制 blob：字节 memcpy + 逐引用叶子 `Value::clone`（值语义） |
| `StructFieldGetPrim dst, base, byte_off, kind` | 读叶子：基元走字节 codec、引用走 `refs` 侧表 |
| `StructFieldSetPrim base, byte_off, kind, val` | 原地写叶子（3a lvalue），同上分流 |

`kind` 是运行期 `TypeTag`（`TAG_I32`/`TAG_STR`/…），给字节宽 + 解码 / 或标识引用叶子。字段 byte
offset / size 由编译期烘焙为**立即数**，运行时无需查表。

### GC：arena 是根，P1 无写屏障

字节 arena 每次采集都作 **GC 根**整体重扫（`scan_roots` 遍历每个 blob 的 `refs`，与 `stack_alloc`
arena 同）→ blob 内引用叶子恒被重标记。因此**写引用进 arena blob 不需写屏障**——写屏障只对「引用写进
**堆对象**」必需（堆对象不作根重扫），即 struct 内联进对象/数组的 **P3**，非本阶段的 P1 局部 struct。

## codegen 翻转（A-use）

`z42c` 的 `ExprEmitter`/`FunctionEmitter` 对 **blob 值 struct**（`StructLayout.IsBlobStruct`：多字段
且各字段非嵌套 struct）发射上述指令：

- `new P(...)` → `StructAlloc` 句柄 + `call ctor(句柄, args)`；ctor body 的 `this.f = a` 因所属类是
  blob struct 翻转为 `StructFieldSetPrim(句柄, offset, tag, a)`，**原地**填 blob（句柄携创建帧
  `frame_id`，跨 ctor 子帧仍解同一 arena 槽）。
- `P b = a`（非 `new`）→ `StructAlloc b` + `StructCopy(b, a)`；`P b = new P(...)` 直接别名 fresh 句柄。
- `b.x = v` → `StructFieldSetPrim`；`a.x` 读 → `StructFieldGetPrim`。
- `this.x` / 裸字段（struct 方法/ctor 内）→ 同上（`this`=reg0 句柄）。

**优化器完整性**：4 条指令的 def/use 必须录入 `IrOptInfo`（`DstId`/`AddReads`/`ReplaceReads`/`SetDst`）
+ 逃逸分析汇点表——漏 `StructFieldSetPrim` 的 `Val` 读 → DCE 误删喂值的 `const`（实测踩坑）。struct
方法暂不入 inline 允许集（`_isInlinable`），保守不内联。

## 嵌套 struct 字段（add-struct-nested-fields）

`struct Line { P a; P b; }`——字段本身是 struct。布局早已递归展平（嵌套 P 的叶子按偏移平移并入 Line
的字节区间 + 引用位图），故 `line.a.x` 的字节地址是**编译期可算的累积 offset**：`off(Line,a)+off(P,x)`。

**准入**：`IsBlobStruct` 去掉"含嵌套 struct 字段即拒"的旧门，改为接受（仍要求 `FieldCount>=2` 且
`Size>0`——后者兜住自引用 struct 的空布局，见下）。

**叶子读写（3a 原地）**：`line.a.x` / `line.a.x = 3` 沿成员链**累积 byte offset**，对根 blob 句柄发射
**单条**现有 `StructFieldGetPrim` / `StructFieldSetPrim`——无新指令、无格式 bump。链根解析两遍互补、
不重复发射：`_structChainRoot` 只 Emit 根一次（局部 / `this` reg0 / 拥有者裸 struct 字段），
`_structChainOffset` 纯查布局表累加偏移。扁平单层 `a.x` 是其退化情形（offset=0），codegen 逐字节不变。

**整字段复制**：`P p = line.a`（读出）/ `line.a = q`（写入）= 对子 struct 的叶子**逐叶子分解复制**
（递归到真叶子；基元走字节 codec、引用叶子走侧表 `get_ref`/`set_ref`），复用现有 Get/SetPrim，
不引入区间复制指令。值语义：`p` 得独立副本，改 `p.x` 不动 `line.a.x`。

**自引用兜底**：`struct Node { Node next; }` = 无限大小（C# `CS0523`）。`LayoutOf` 的 `_inProgress`
环检测置 `ErrorType` 并返回空布局（`Size==0`）→ `IsBlobStruct` 的 `Size==0` 门拒之 → 退化引用语义
（与今日一致、不崩）。显式 `E0438` 诊断留 follow-up。

## struct 值相等（`==` / `!=`，add-struct-value-equality）

blob 值 struct 的 `==` / `!=` 是**字段级值相等**，而非句柄身份。若不脱糖，两操作数持
`Value::StructRef{idx, frame_id}` 句柄，VM 的 `Eq` 比 arena 下标 → 字段完全相同的两个 struct 恒判不等。

**脱糖（纯前端、无新指令、无格式 bump）**：`ExprEmitter._emitBinary` 检测 `==`/`!=` 两操作数均
`IsBlobStruct` 时，分流到 `_emitStructEquality`——操作数**各求值一次**（`a`/`c` 为 blob 句柄，避免
`f()==g()` 重复求值），`_emitLeafEqChecks` 递归展平叶子（镜像 `_copyRegion`：嵌套 struct 字段递归累积
offset），每个真叶子发射两条现有 `StructFieldGetPrim` + 一条现有 `Eq` + `BrCond` 短路——任一叶子不等
即跳共享 `seq_ne` 失败块。结果 `result` 寄存器在「全等」与「fail」两分支各写 `ConstBool`，end 块读汇合
（镜像三目 `_emitConditional`）。`!=` 只是翻转两分支的 `ConstBool`（全等→false / fail→true）。

**叶子比较语义完全复用现有 `Eq`**——基元→值相等（**float NaN → false**，符合 `==` 运算符语义）；
`string` 叶子→**内容相等**（`Arc<str>` deref 比较）；`object`/`array` 叶子→**引用相等**（符合 z42 对象
`==` 默认 + C# `ValueType.Equals` 对引用字段的行为），不递归深比较堆对象。

```
p1 == p2   ⟹   逐叶子: la=field_get(p1,off,tag); lc=field_get(p2,off,tag); cmp=eq(la,lc)
                        br.cond cmp → 下一叶子块 / seq_ne(失败)
               全叶子相等 → result=const true;  seq_ne → result=const false
```

> 仅拦截 `==`/`!=` 且两侧均 blob struct；`<`/`<=`/`>`/`>=` 对 struct 无序（类型检查器不允许），非 blob
> 操作数（基元/引用类型/单叶子 wrapper）走原 `_emitCompare` 不变。**衔接**：`_emitLeafEqChecks` 确立的
> 逐叶子值相等，就是未来 struct 合成 `Equals`（C# `ValueType.Equals`）/ boxed struct 相等要复用的语义
> （struct→object 装箱见 Deferred P4）。

## struct→object 装箱 + 身份（add-struct-object-boxing PR2a）

值 struct 是 C# 真值类型：不形式继承 `Object`、无 vtable（`z42.core/Object.z42` 契约）。要当 `object`
用（赋给 `object` 变量 / 参数 / 数组、`is`/`as`/`GetType`）靠**装箱**桥接——把帧作用域 blob 拷到堆稳定
表示，而非给值类型加 vtable。

**修的真 bug**：`object o = someStruct` 类型合法（`TypeFactsTc._isAssignable` 的「任何类型可赋给 object」
规则）但装箱缺失时**裸拷帧作用域 `Value::StructRef` 句柄进 object 槽**——创建帧一退出（arena LIFO
truncate）即 use-after-free。

**堆表示**：新 `Value::BoxedStruct(Box<BoxedStructData{type_name, bytes, refs}>)`——**拥有** blob 字节快照
+ 引用叶子（作真 `Value`，GC 扫描；`is_heap_ref`=true 触发写屏障）+ FQ 类型名。**不**用 ScriptObject
（那需给 struct 加 base+vtable=反转无-vtable 决定）。`size = bytes.len()`。

**装箱**（`__box_struct` builtin，复用 `Builtin` opcode → 无格式 bump，同 `__box_prim`）：`TypeChecker.BoxIfNeeded`
对 blob 值 struct 擦除到 `object`/接口插 `BoundBox`；`ExprEmitter._emitBox` 发 `__box_struct(structHandle)`；
VM 从 arena slot 拷 `bytes`+clone `refs`+类型名（类型名从 slot 取，无需 class 参数）→ 堆 `BoxedStruct`
（值快照，脱离帧）。

**拆箱**（`(P)o`）：C 风格强转 `(T)x` 绑 `BoundConvert`；`_emitConvert` 见「目标 blob struct ∧ 源非
struct（object/boxed）」→ 发 `AsCast`（复用现有 opcode）。VM `as_cast` 对 `BoxedStruct` 精确类型匹配 →
`unbox_struct`：在**当前帧** arena alloc + 拷 bytes/refs → 返回值 struct `StructRef`（独立副本）。

**身份**：`is_instance` / `as_cast` / `builtin_obj_get_type`（interp + JIT helper 对称）加 `BoxedStruct`
分支——`is P`/`is object` true、`GetType()` → 精确 struct `Type`（type_name 驱动）、`as P` 拆箱 /
`as object`·base·接口 保持 boxed / 不匹配 Null。`o.GetType()` 经 VCall 的 `BoxedStruct` 分支特判到
`builtin_obj_get_type`（保留精确类型，不拆箱 this）。

> **JIT**：JIT 对 struct 值指令一律 bail→interp（Phase D），故用 struct 值的函数跑 interp（拆箱在此健全）。
> JIT 帧无 `frame_id` 不能产 arena `StructRef`，故 `jit_as_cast` 对 boxed struct 命中即保持 boxed、绝不产
> 无效句柄（可消费的拆箱结果必含 struct 指令 → 整函数回退 interp）；`jit_is_instance`/`jit_vcall`(GetType)
> 加 `BoxedStruct` 分支（身份，无 alloc）与 interp 对称。

## struct 合成对象协议方法（add-struct-object-methods PR2b）

落地 `z42.core/Object.z42` 契约「compiler synthesises value-semantic Equals/GetHashCode/ToString」——boxed
struct 的完整对象协议。unboxed struct 仍无 vtable（这些方法经装箱后的对象协议 / 名字派发，非 vtable）。

- **`Equals(object)`**：**编译器合成** IR 函数 `{FQ}.Equals$1`（`IrGen` 类成员循环末尾注入，与合成 ctor
  同位；用户显式声明则不合成；`build_func_index` 按名注册）。body（`FunctionEmitter.EmitSynthStructEquals`
  → `ExprEmitter.EmitSynthEqualsResult`）= `(other is P) ? leafEq((P)this,(P)other) : false`——**this/other
  均按 boxed 处理、内部 `AsCast` 拆箱到 callee 帧 arena StructRef**（避开 JIT 帧无 frame_id），再复用 PR1
  `_emitStructEquality` 逐叶子比较（NaN 精确、嵌套递归、string 内容 / object 引用）。
- **`GetHashCode()`**：**native `__struct_hash_code`**（VM boxed-vcall 臂路由）——对 boxed blob 的 `bytes`
  FNV-1a + 混入引用叶子哈希（string 内容；object/array 叶子弱贡献常量，因 Equals 对引用叶子按引用比较）。
  `& 0x7fffffff` 非负（Dictionary 契约）。同值 → 同 `bytes`/`refs` → 同哈希。
- **`ToString()`**：VM boxed-vcall 臂直接返回**短类型名**（C# `ValueType.ToString` 默认；字段 dump 留后续）。
- **`GetType()`**：`builtin_obj_get_type`（PR2a）。

**VM 派发**（`exec_vcall.rs` + `jit/helpers/vcall.rs` BoxedStruct 臂，interp+JIT 对称）：`GetType`/`GetHashCode`/
`ToString`（arity 0）→ native 特判；否则 prepend `{type_name}.{method}$arity` 候选命中合成/用户方法（this=boxed
值，合成 body 内拆箱），fallback `Std.Object.{method}`。

**D5 定案**：`==`/`!=` on `object`-typed boxed struct = **值相等**（`Value::BoxedStruct` `PartialEq`：
type_name∧bytes∧refs），延续 ② 对 struct `==` 的值语义（z42 boxed 是 owned Box 非共享 GcRef，C# 的装箱引用
相等在此表示下 ill-defined）；`.Equals()` = 合成叶子方法（float `Eq` → NaN≠NaN 精确）。**边角**：float NaN
`==` 按位判等 vs `.Equals` 浮点== → 极少含 NaN 的 struct 二者微差（pre-1.0 标注，要完全一致须让 `==` 也走
vcall Equals，代价不值）。

**Deferred**：
- **struct 作泛型容器键**（`Dictionary<P,V>`/`HashSet<P>`）+ **VCall on 未装箱 StructRef receiver**——泛型路径
  把 struct 键当未装箱 StructRef 传入（`key.GetHashCode()`=对 StructRef 的 VCall，且存进容器堆数组=帧作用域
  句柄逃逸 use-after-free）。正确解 = **泛型边界装箱** / P3 容器内联（**PR2b 前本就不工作**，非回归）。
- ToString 字段 dump；`IEquatable<T>.Equals(P)` typed 重载；反射 GetMethods 报告合成方法（SIGS 元数据，
  可选、动 SIGS 有自举字节稳定性风险，留后续）。

## struct 泛型容器装箱（add-struct-generic-boxing P3a）

`Dictionary<P,V>` / `List<P>` / `HashSet<P>` 存 struct 键/值/元素——**泛型边界装箱**（非字节内联；密度内联
是 P3b）。格式中立：复用 `__box_struct`（存）+ `AsCast`（取）+ `as_cast` 的 StructRef 恒等臂，容器 backing
（`TKey[]/T[]`，运行期擦除）与 ABI 不变。

**问题**：泛型路径把 struct 实参当**未装箱 `Z42GenericParamType`（K/T）** 传入——`BoxIfNeeded` 只对
`object`/接口目标装箱，type-param 不装箱 → 裸 `StructRef`（帧作用域 arena 句柄）流入容器：`Dictionary.Set`
的 `key.GetHashCode()` = 对 StructRef 的 VCall → 崩；`keys[slot]=key` 存进堆数组 → 帧退出 use-after-free。

**装箱（存入）**：`TypeChecker.BoxIfNeeded` 的 `erasesS` 谓词加 `|| (target is Z42GenericParamType)`——
覆盖所有走 `BoxArgs` 的方法实参（`List.Add`/`Dictionary.Set`/`Contains`…）。`d[key]=v` 的 indexer-set
（`ExprTyper._bindAssign` 手搭、**绕过 BoxArgs**）与 `d[key]` 读的 get_Item 索引实参（`_bindIndex`）单独按
`set_Item`/`get_Item` 的 `ParamTypes` 装箱。→ 容器存 `Value::BoxedStruct`（堆稳定），`GetHashCode`/`Equals`
走 PR2b 的 boxed-vcall 臂。

**拆箱（取出）**：取回到具体 struct 类型需拆回值 struct。`TypeChecker.StructUnboxTarget` 判「泛型返回
（get_Item / 方法返回 T）subst 后是否 blob struct」，是则调用点把结果包 `BoundConvert(→P)`，复用
`ExprEmitter._emitConvert` 的 `AsCast` 拆箱臂。`foreach (P p in list)` 在 `FunctionEmitter` 对元素发 `AsCast`。

**`as_cast` 的 StructRef 恒等臂**（关键统一点）：泛型容器迭代/取值统一走 `AsCast`，但元素运行期可能是
`BoxedStruct`（泛型容器，Add/set 装箱）**或**已是 `StructRef`（普通 `P[]`）——静态同为 `P[]` 不可辨。故 VM
`as_cast`/`jit_as_cast` 加 **StructRef 源 → 原样返回**（已是值 struct，`as P` 恒等；编译器仅在静态类型即该
struct 处发此 AsCast），使两种运行期种类统一：`BoxedStruct`→拆箱 / `StructRef`→恒等。取出的 struct 是拷到
当前帧 arena 的**独立副本**（值语义：改它不动容器）。

**Deferred → P3b**：真**字节内联**进堆对象字段 / `struct[]` backing（密度 + FFI）+ 写屏障——本 P3a 只装箱，
容器里是 boxed 堆对象，非内联字节。

## struct 内联进堆对象字段 + struct[] backing（add-struct-heap-inline P3b）

P3a 让 struct 进容器靠**装箱**（每元素一个堆 `BoxedStruct`，无密度）。P3b 让 struct 值**字节内联**进
**堆对象字段**（`class C { Point pt; }`）与 **`Point[]`**——真密度（基元字节精确打包，逼近 C# 布局）+
FFI 零 marshaling + 零 per-field 堆分配。这是 struct 值语义功能面的闭合项。

### Decision D1-a：基元内联 + 引用叶子侧表（非裸内联）

内联 struct 的引用叶子（string/object/array）怎么存，是核心设计分叉。选定 **D1-a**：
- **基元叶子**按字节精确**打包进对象字节区** `ScriptObject::struct_bytes`（密度/FFI 收益全在此）；
- **引用叶子**走对象的 `struct_refs: Box<[Value]>` **侧表**（真 `Value`），**不裸内联** 16B 句柄进字节区。

> **为什么不裸内联引用叶子（否决 D1-b）**：GC 访问协议是 `visitor(&Value)`，`Value` enum 远大于 16B 且带
> 判别式——无法只存 16B 句柄再还原完整 `&Value`；`Arc<str>` 裸字节要手工 `ManuallyDrop`/`Arc::from_raw`
> 管引用计数，漏一处即 double-free。而引用叶子无论放侧表还是字节区**都是 16B 句柄，密度无差**。故侧表既拿
> 全部密度收益、又换回内存安全 + 与 arena `StructSlot`/`BoxedStruct` 完全同构（`StructCopy` 无转码）。

### 对象内联表示与访问（路线 α）

`ScriptObject` 加 `struct_bytes`（内联字段基元打包）+ `struct_refs`（引用叶子侧表）。`TypeDescCold.inline_layout`
= 类的**合成内联布局**（对象相对字节区 size + 引用位图，复用 `StructTypeLayout`——对象内联区 = 字节 blob +
引用侧表，与 struct 同构）。alloc 时零初始化（= struct 默认值）。**内联字段仍保留一个 dead slot**（不重排
`field_index`/slots，最简；真数据只在 struct_bytes，dead slot 恒 Null；1 slot/字段小浪费留 P4/P5）。

访问复用现有 `StructFieldGetPrim/SetPrim`（0xC0–0xC3，**无新 opcode**）——`base` 从「仅 arena StructRef」扩到
「也可为堆 `Value::Object`」：叶子基元读写 `obj.struct_bytes[byte_off]`（`byte_off` = 编译期烘焙的对象相对
复合 offset `off_field + off_leaf`）；引用叶子读写 `obj.struct_refs[inline_layout.ref_index(byte_off)]`。

### GC：扫描 + 写屏障（P3b 核心）

内联 struct 的引用叶子落在堆对象字节区内，**不再是独立 GC 根重扫**（arena 每采集重扫故无屏障，见上「P1 无
写屏障」）——堆里的内联叶子需两件事：
- **扫描**（mark 追踪）：`scan_object_refs`/`trace_children` 的 `Object` 臂遍历 `obj.struct_refs`（与
  `BoxedStruct.refs` 一行同构，零 unsafe）——D1-a 侧表让这平凡复用 `visitor(&Value)`；
- **写屏障**（并发/分代正确性）：写内联引用叶子 = 写 `struct_refs[k]` 一个 `Value` 槽 → 复用现有
  `write_barrier_field(owner, k, new)`（STW 默认 no-op）。**无新屏障机制**——这是 D1-a 相对裸内联最大的工程简化。

### 格式 wire：内联字段表（zbc 1.32 / zpkg 0.37）

类描述符尾部加**合成内联布局块**（`CLASS_FLAG_HAS_INLINE_STRUCT` bit7=0x80 gated，紧随 struct 块）：
`size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n`——同 struct 块 shape（reader 复用 `StructLayoutDesc`）。
writer 侧 `ClassDescBuilder` 用 `StructLayout.InlineLayoutOf`（`BuildFromSymbols` 为每个非-struct class 预计算，
**writer 与 codegen 同源取对象相对 offset → 一致**）。字段 byte offset 由 codegen 烘焙进访问指令，不入块。

### codegen 翻转（对象字段，已落）

`ExprEmitter` 谓词 `_isInlineStructFieldRoot`（字段类型 `IsBlobStruct` ∧ 容器是 class）+ `_isOwnerInlineField`
（class 方法内裸 `pt`=this.pt，靠 `EmitContext.OwnerClassName`）。`_structChainRoot`/`_structChainOffset` 扩两
根（内联字段根 = 对象句柄 / reg0）→ 叶子 `c.pt.x`/`pt.x` 复用嵌套链发 `StructFieldGetPrim/SetPrim`；整字段读
（`Point p = c.pt`）→ `StructAlloc` + `_copyRegion` 拷出（值副本）；整字段写（`c.pt = q`）→ `_copyRegion` 拷入。

### `struct[]` 字节 backing（add-struct-array-codegen，P3b follow-up）

`ArrayBacking::StructBytes{elem_size, bytes, refs, layout}`（C# inline `struct[]`：元素基元紧凑
`bytes[len*elem_size]` + 引用叶子并行 `refs[len*ref_count]`）。`arr[i]` 元素 offset 运行期定 → 需**堆 base 句柄**
`Value::StructRefHeap(Box<StructArrayElem{arr, index}>)`（arena `StructRef` 热路径不动；仅数组需句柄）。GC：
`ArrayObj::gc_refs()` 统一 `Boxed ∪ StructBytes.refs` 供扫描；元素引用叶子写触发 `write_barrier_array_elem`。

- **创建**：`array_new`/`array_new_lit` 对 **blob 值 struct 元素**（`try_struct_backed`：`TypeDesc.fields≥2` +
  `struct_layout`，匹配编译期 `IsBlobStruct`）造 `StructBytes` backing（`ArrayObj::struct_backed`，经
  `Heap::alloc_array_obj` region-alloc 保 backing）；字面量经 `pack_struct_elem` 把各元素（`StructRef` 经 arena /
  `BoxedStruct`）字节+引用叶子拷进元素槽。
- **取值**：`array_get` 对 `StructBytes` backing 产 `StructRefHeap` 元素句柄（有 array `GcRef`，替代 `get_boxed`）。
- **codegen（ExprEmitter）**：`_emitArrayElemHandle`（ArrayGet 直发句柄不拷贝）；`_emitIndex` 对 struct[] 出
  `StructAlloc`+`_copyRegion` 拷出（standalone `arr[i]` 值副本）；`_structChainRoot` 对 BoundIndex struct[] 根=句柄
  （`arr[i].x` 原地叶子读写复用嵌套链发 `StructFieldGetPrim/SetPrim`）；`arr[i] = p` 走句柄+`_copyRegion` 拷入。**无新 opcode、格式中立。**

### 已工作 / Deferred

- ✅ **对象内联 struct 字段**（`class C { Point pt; }`）：默认零初始化 / `c.pt.x` 叶子读写 / 整字段拷入拷出值语义
  独立 / 方法内裸字段 / string 引用叶子内联 / 多对象独立——golden `struct_heap_inline.z42` 端到端验证。
- ✅ **`struct[]` 值类型数组**（`Point[]`）：默认零初始化 / `arr[i].x` 叶子读写 / 整元素拷出拷入值语义独立 /
  元素独立 / `new Point[]{}` 字面量 / string 引用叶子内联——golden `struct_array.z42` 端到端验证。格式中立。
- ✅ **class 实例方法返回 struct**（`Point GetPt(){ return pt; }`）：`_emitCall` instance 分支返回 blob struct 时
  三派发路径（devirt 直 Call / DepIndex Call / VCall fallback）均追加返回 blob 句柄作**末尾隐藏 sret 实参** + void
  dst；object VCall 按 vtable slot(方法名) 派发 arity 不入解析键 → 不破派发。golden `struct_heap_inline.z42`（GetPt）验，格式中立。
- ⏳ Deferred：**foreach over struct[]**（`foreach(P p in arr)` 元素拷出，索引循环已工作）、
  **JIT 值路径**（P5）、**跨包内联布局 + 反射**（P4）。

## 与逃逸分析 / packed 数组的关系

- struct 恒内联，**不走** `ObjNew`→堆/`StackObject` arena（Decision θ）；逃逸 arena 是**引用类型**的
  分配优化，struct 内联是**值类型**的语言语义——两套机制。
- 字节 blob 地基与 [packed-primitive-arrays] 的字节 `ArrayBacking` 收敛（P3 的 `struct[]` 字节 backing）。

## 收敛面与延后

- ✅ 局部多字段扁平 struct：构造 / 复制 / 字段 get·set / `this` 字段 / 传参 copy-in / 返回值 sret（A-use）。
- ✅ **嵌套 struct 字段**（`Line{a:P}`）：累积-offset 叶子读写（3a）+ 整字段逐叶子复制（add-struct-nested-fields）。
- ✅ **`struct==` 值相等**（`==` / `!=`）：逐叶子值比较脱糖，复用现有 `StructFieldGetPrim` + `Eq` + `BrCond`
  短路——**无新指令、无格式 bump**（add-struct-value-equality）。
- ✅ **struct→object 健全装箱 + 身份**（`GetType`/`is`/`as`，blob 拷到堆稳定表示，修裸拷 StructRef 悬垂；
  add-struct-object-boxing PR2a）。
- ✅ **struct 合成对象协议方法**（boxed `Equals` 值相等复用 PR1 / `GetHashCode` native FNV / `ToString`
  类型名 / `==` on boxed 值相等 D5；add-struct-object-methods PR2b）。
- ✅ **struct 作泛型容器键/值/元素**（`Dictionary<P,V>` / `List<P>` 存取·ContainsKey·foreach·Contains）：
  **泛型边界装箱**（存入 type-param 装箱、取出到具体 struct 拆箱），复用 `__box_struct`+`AsCast`+`as_cast`
  StructRef 恒等臂——**格式中立、容器 ABI 不变**（add-struct-generic-boxing P3a）。
- ✅ **struct 真内联进堆对象字段**（`class C { Point pt; }`，P3b add-struct-heap-inline）：基元字节内联进
  `struct_bytes` + 引用叶子 `struct_refs` 侧表（D1-a）+ 复用 `StructFieldGetPrim/SetPrim` 对象 base（路线 α）+
  GC scan/`write_barrier_field` 复用侧表 + 格式 wire 内联字段表（zbc 1.32/zpkg 0.37）。golden 端到端验证。
- ✅ **`struct[]` 值类型数组元素 codegen**（add-struct-array-codegen）+ **class 实例方法返回 struct**
  （add-struct-method-return，`sret × VCall`）——均格式中立，golden `struct_array.z42` / `struct_heap_inline.z42`(GetPt) 验。
- ⏳ Deferred：**foreach over struct[]**（索引循环已工作）、**单标量叶子 struct 塌缩**（`GCHandle`=Phase B）、
  **跨包布局元数据 / 反射合成方法可见**（P4）、**JIT 值路径**（P5，现全 bail→interp）、**ToString 字段 dump**、
  **E0438 自引用诊断**（现 `Size==0` 兜底防崩）。
