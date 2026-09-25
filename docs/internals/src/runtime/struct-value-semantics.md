# struct 值语义（内联字节 blob）

> 状态：A-use 落地（2026-08-09，zbc 1.31 / zpkg 0.36）；嵌套字段 + `struct==` 值相等（2026-08-10）+
> struct→object 装箱身份 + 合成对象协议方法 + 泛型容器装箱（2026-08-11）+ 堆内联字段 / `struct[]` /
> 方法返回 struct / foreach（2026-08-11，zbc 1.32 / zpkg 0.37）+ **JIT 值路径 helper 桥接 + 跨包 struct
> 值语义（P4a）+ 装箱引用身份 + struct 字段反射（P4b）**（2026-08-12，均格式中立）+ **基元装箱统一到
> `BoxedStruct`（unify Phase 2 R3，2026-08-13，格式中立）+ **泛型（实例化）值 struct 值相等边界修复
> （fix-generic-struct-erasure-boxing，2026-09-01，格式中立）**落地。本页讲**多字段 struct 的真值语义**如何在编译器 + 运行时
> 实现。程序全景（选项 B / B-radical 统一值类型 / 分阶段）见 `docs/spec/changes/add-struct-value-semantics/`。

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
- **属性 getter 读（`x.Prop`，`Prop` 是 `T Prop { get {...} }`）→ 静态 Call `<Struct>.get_Prop`（传 blob
  句柄，sret-aware），不是 `StructFieldGetPrim`**（fix-struct-property-getter）。判据：成员有 `get_Prop`
  方法即属性（`MemberCollector` 把属性名也登记进 `Fields` 供类型检查，但计算属性无 byte-layout 存储、
  auto-property 存储在 `__prop_Prop` 而非源名——故不能按源名查字段偏移）。**镜像 class 属性 getter 的
  `AccessEmitter._emitMember` 判据（只查 `Methods` 有无 `get_X`，不查 `Fields`），只是 struct 无虚方法 → 走
  静态 Call 传 handle 而非 `VCall`**（VCall on `StructRef` receiver 会崩「expected object, got StructRef」，
  同 struct 实例方法调用）。历史坑：曾把 struct 成员一律当字段发 `StructFieldGetPrim`，属性名查布局落空得
  offset `-1` → 运行期 `struct ref leaf at byte offset 4294967295`。

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

**链节必须真内联（fix-generic-struct-chain-access, 2026-09-15）**：累加偏移的前提是「这一节的字节就在
容器 blob 里」。判据是 `AccessEmitter._isInlineChainLink`：容器是 blob struct **且** 该字段在容器布局里
`FieldIsStruct`。**只看容器是 blob struct 不够**——泛型 struct 的布局按**定义**算，字段声明类型是 `T`，
擦除成一个**引用叶子**（存另一块 blob 的句柄）；实例化后这一节的静态类型虽是 struct
（`ValueTuple2<int,string>`），存储却不在容器里：

```text
((int,string),int) t            ValueTuple2 布局（按定义 T1,T2）
                                ┌────────────┬────────────┐
                                │ Item1 : T1 │ Item2 : T2 │   两个引用叶子
                                └─────┬──────┴────────────┘
                                      └──► 另一块 blob (int,string)

t.Item1.Item2  旧：off(VT2,Item1)+off(VT2,Item2) 在 t 的 blob 上读 ⇒ 读到 t.Item2（静默错值）
               新：Item1 非内联 ⇒ 断链：先取 t.Item1 的句柄为根，再在它上面读 Item2（偏移从 0 起）
```

修复前的症状：`t.Item1.Item2` 读出外层 `Item2`、`t.Item1.Item1` 读出整块内层句柄后装箱崩、
`pp.First.Y = 5` 写进外层别的字段。**先读进局部**（`var x = t.Item1; x.Item2`）一直是对的——
单节读正好走「非内联字段 = 取句柄」路径。属性 getter 出现在链中间（无布局存储）也按同一判据断链。
读写共用 `_structChainRoot` / `_structChainOffset`，故读、写、复合赋值一起修正。golden
`src/tests/types/generic_struct_chain.z42`（interp + jit）。

> ✅ **已不再别名**（generic-struct-erased-slot-value-copy）：命中闸门的实例化拿到自己的布局，
> 型参字段是**真内联字节**而非句柄，故 `pp.First.Y = 5` 写的就是 `pp` 独占的那段字节。
> 闸门外（跨包实例化 / 布局与定义相同者）仍是擦除句柄表示——见下方该条目的「仍未覆盖」。

**整字段复制**：`P p = line.a`（读出）/ `line.a = q`（写入）= 对子 struct 的叶子**逐叶子分解复制**
（递归到真叶子；基元走字节 codec、引用叶子走侧表 `get_ref`/`set_ref`），复用现有 Get/SetPrim，
不引入区间复制指令。值语义：`p` 得独立副本，改 `p.x` 不动 `line.a.x`。

**自引用兜底**：`struct Node { Node next; }` = 无限大小（C# `CS0523`）。`LayoutOf` 的 `_inProgress`
环检测置 `ErrorType` 并返回空布局（`Size==0`）→ `IsBlobStruct` 的 `Size==0` 门拒之 → 退化引用语义
（与今日一致、不崩）。显式 `E0438` 诊断留 follow-up。

## struct 值相等（`==` / `!=`，add-struct-value-equality）

blob 值 struct 的 `==` / `!=` 是**字段级值相等**，而非句柄身份。若不脱糖，两操作数持
`Value::StructRef{idx, frame_id}` 句柄，VM 的 `Eq` 比 arena 下标 → 字段完全相同的两个 struct 恒判不等。

**脱糖（纯前端、无新指令、无格式 bump）**：`OperatorEmitter._emitBinary` 检测 `==`/`!=` 两操作数均
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

**堆表示**（PR2a 原始；**P4b 已改为共享 `ScriptObject`**，见下「装箱引用身份」节）：PR2a 用
`Value::BoxedStruct(Box<BoxedStructData{type_name, bytes, refs}>)`——**拥有** blob 字节快照 + 引用叶子
（作真 `Value`，GC 扫描）+ FQ 类型名（值语义 `Box`，无引用身份）。**P4b 把载荷改为 `GcRef<ScriptObject>`**
（共享堆句柄，struct blob 存进对象 `struct_bytes`/`struct_refs`）→ 对齐 C# 引用身份，复用 `region_object`。
两版都**不**给 struct 加 base+vtable（无-vtable 决定不变），只是把值装进对象容器。

**装箱**（`__box_struct` builtin，复用 `Builtin` opcode → 无格式 bump，同 `__box_prim`）：`TypeChecker.BoxIfNeeded`
对 blob 值 struct 擦除到 `object`/接口插 `BoundBox`；`TypeOpEmitter._emitBox` 发 `__box_struct(structHandle)`；
VM 从 arena slot 拷 `bytes`+clone `refs`+类型名（类型名从 slot 取，无需 class 参数）→ 堆 `BoxedStruct`
（值快照，脱离帧）。

**拆箱**（`(P)o`）：C 风格强转 `(T)x` 绑 `BoundConvert`；`_emitConvert` 见「目标 blob struct ∧ 源非
struct（object/boxed）」→ 发 `AsCast`（复用现有 opcode）。VM `as_cast` 对 `BoxedStruct` 精确类型匹配 →
`unbox_struct`：在**当前帧** arena alloc + 拷 bytes/refs → 返回值 struct `StructRef`（独立副本）。

**身份**：`is_instance` / `as_cast` / `builtin_obj_get_type`（interp + JIT helper 对称）加 `BoxedStruct`
分支——`is P`/`is object` true、`GetType()` → 精确 struct `Type`（type_name 驱动）、`as P` 拆箱 /
`as object`·base·接口 保持 boxed / 不匹配 Null。`o.GetType()` 经 VCall 的 `BoxedStruct` 分支特判到
`builtin_obj_get_type`（保留精确类型，不拆箱 this）。

> **JIT**（P5-A 起，见下「JIT 值路径」节）：`jit_as_cast` 对 boxed struct 精确匹配**拆箱**到当前帧
> arena `StructRef`（`frame_id` 惰性分配）；`as object`·base·接口保持 boxed。`jit_is_instance`/
> `jit_vcall`(GetType) 的 `BoxedStruct` 身份分支（无 alloc）与 interp 对称。（P5 前 JIT 对 struct 值指令
> 一律 bail→interp，`jit_as_cast` 命中即保持 boxed——该限制已解除。）

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

### 泛型（实例化）值 struct 的值相等边界（fix-generic-struct-erasure-boxing, 2026-09-01）

合成 `Equals(object)` 的 body 首指令是 `other is P`（`is_instance`）——**只有 `other` 是 `BoxedStruct` 时**
runtime 才认（`is_instance` 无裸 `StructRef` 臂）。因此调用点**必须把 struct 实参装箱到 `object`**，否则
类型测试失败 → 直接走 else 返 `false`（值明明相等）。泛型 record struct（`GRec<int,int>` / `ValueTuple2<int,int>`）
此前踩两个独立缺口：

1. **实参装箱漏 `Z42InstantiatedType`**（`TypeChecker.BoxIfNeeded`）：擦除边界装箱判据只认 `Z42ClassType`，
   而**泛型实例化** struct 的静态类型是 `Z42InstantiatedType` → 漏装箱 → 实参裸 `StructRef` 传入 `Equals(object)`。
   修：unwrap `Z42InstantiatedType.Def` 再判 `IsStruct`，装箱名用 `.Def.Name()`（擦除基名，与合成 `{FQ}.Equals$1`
   注册名 + runtime `type_desc` 一致）。**本地**用户泛型 record struct 由此修复。

2. **跨包同签名歧义**（`OverloadBinder._collectOverloads`）：**imported** record struct（如 stdlib `ValueTuple2`）
   的合成 `Equals$1(object)`（RegKey `Equals$1`）跨包可见，与继承的 `Object.Equals(object)`（RegKey `Equals`）
   **签名相同**。旧的**按 RegKey** 去重令二者并存 → 同签名两候选 → 重载决议歧义（伴随伪 E0425）→ 解析失败
   → `.Equals` 松绑 `Unknown` **不经 `BoxArgs` 装箱** → 运行期同 ① 症状。修：`_collectOverloads` 改**按有效签名**
   （简单名 + 形参规范类型，`_overloadSigKey`）跨基链去重——派生类型自己声明/合成的方法**隐藏**基类同签名方法
   （C# method hiding，基链自派生向基遍历、首见者胜）。合成 `Equals$1` 隐藏 `Object.Equals` → 唯一候选 → 正常
   装箱决议。type-based 重载（形参类型不同 → 签名各异）与 `override`（同 RegKey 同签名）行为不变。

   > **本地泛型 struct 不踩 ②**：本地 struct 的合成 `Equals$1` 在类型检查**之后**注入 `IrGen`，类型检查期不可见
   > → `_collectOverloads` 只见继承的 `Object.Equals` → 单候选、无歧义。歧义只在合成方法已随 zpkg 导出、对
   > consumer 可见的**跨包**场景出现。

### 编译器派发：值类型 receiver 的 Object 方法（fix-value-type-object-methods, 2026-09-01）

上面是**运行期**协议；但**编译器**此前不把值类型实例的 Object 方法调用路由到它——`a.GetType()`/`a.ToString()`
一律发**静态 `Call {Struct}.{method}`**（blob-struct 分支），Object 方法在 struct 上无函数体 → 运行期
`undefined function`。`E.Red.GetType()` 则因 enum 值是裸 i64 → `primitive_class_name(I64)` → 错误的 `Std.Int32`。
补齐后 `CallEmitter._emitCall` 的 instance 分支按方法分两路（覆盖 struct + enum，含空/单字段 struct 与 scalar
之外的值类型）：

- **`GetType()`（0 参）→ 折叠 `typeof(静态类型)`**（`_emitValueTypeGetType`）。值类型 **sealed**（无多态）→
  编译期静态类型 == 运行期类型，GetType 结果编译期已知；发 `TypeofInstr(FQN)`（复用 typeof codegen，真句柄），
  **无装箱开销**。判据 `_isStructOrEnumStatic`：`Z42ClassType.IsStruct && !IsScalarValue`（用户 struct；scalar
  基元的 `IsStruct` 亦 true，但 `5.GetType()` 已由运行期正确处理，故排除以免自举字节漂移）或名在 `EnumTypes`
  表（enum 变量 receiver，其 `Z42ClassType.IsStruct=false`）。**enum 成员引用 `E.Red`** 的载体仍是
  `BoundLitInt`（codegen 照发整数字面量），绑定时打的 `EnumTypeName` **origin 标记**让 `CallEmitter`
  折叠回 `typeof(E)`。
  > ⚠️ 本段原先写「`E.Red` 静态类型是 `long`（z42 的 **enum-as-int 模型**：`E.Red == 0` / 传 int 参
  > 不变）」——**该模型已于 `make-enum-distinct-type`（2026-09-09）废除**。`E.Red` 的静态类型现在
  > 就是 `E`；`E.Red == 0` 与「传 int 参」都不再成立（双向都要显式 cast）。**载体是 `BoundLitInt`、
  > 运行期表示是 i64** 这两点不变，变的是**身份**：擦除到 `object` 时装箱成挂 enum 自己 `TypeDesc`
  > 的盒，`GetType()` 因此与这里的编译期折叠答案一致。
  > enum 的完整语义 SoT 见 [`language/enums.md`](../../../reference/src/language/enums.md)。
- **`ToString`/`Equals`/`GetHashCode`（struct 未自声明时）→ `__box_struct(recv)` 装箱 + VCall**
  （`_emitBoxedStructObjectCall`），命中上面的 runtime 装箱-struct 协议。**自声明**（record 合成 / 用户覆写，
  `EmitContext.ChainHasMethod` 命中）仍走各自静态 `Call`——保 record 的 `ToString`（`R { A = 1, B = 2 }`）/
  值 `Equals` 与用户 `ToString` 不被装箱短名拦截。

> **struct 导出方法表仍不注入 Object 四方法**（`ClassExtractor`，decision 3 选项 A）——保 zbc TYPE 段元数据 /
> 自举字节不变。这只决定 `typeof(struct).GetMethods()` **是否列出** Object 四方法（反射枚举完备性维度，收益边际、
> blast-radius 大，留后续），**不影响调用**——调用由上面的 `CallEmitter` 路由解决。（旧注释误称「镜像 C#
> ExcludeFromImplicitObject」，实则 C# struct 经 `ValueType:Object` 有这些方法，已订正。）

**D5 定案**：`==`/`!=` on `object`-typed boxed struct = **值相等**（`Value::BoxedStruct` `PartialEq`：
type_name∧bytes∧refs），延续 ② 对 struct `==` 的值语义（**P4b 后**：`PartialEq` 先 `ptr_eq`（同盒短路，避免
双 borrow 死锁），否则经共享对象比 `struct_bytes`/`struct_refs`——值相等语义不变）；`.Equals()` = 合成叶子方法
（float `Eq` → NaN≠NaN 精确）。**边角**：float NaN
`==` 按位判等 vs `.Equals` 浮点== → 极少含 NaN 的 struct 二者微差（pre-1.0 标注，要完全一致须让 `==` 也走
vcall Equals，代价不值）。

**Deferred**：
- **struct 作泛型容器键**（`Dictionary<P,V>`/`HashSet<P>`）+ **VCall on 未装箱 StructRef receiver**——泛型路径
  把 struct 键当未装箱 StructRef 传入（`key.GetHashCode()`=对 StructRef 的 VCall，且存进容器堆数组=帧作用域
  句柄逃逸 use-after-free）。正确解 = **泛型边界装箱** / P3 容器内联（**PR2b 前本就不工作**，非回归）。
- ToString 字段 dump；`IEquatable.Equals(P)` typed 重载；反射 GetMethods 报告合成方法（SIGS 元数据，
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
（`AssignTyper._bindAssign` 手搭、**绕过 BoxArgs**）与 `d[key]` 读的 get_Item 索引实参（`ExprTyper._bindIndex`）单独按
`set_Item`/`get_Item` 的 `ParamTypes` 装箱。→ 容器存 `Value::BoxedStruct`（堆稳定），`GetHashCode`/`Equals`
走 PR2b 的 boxed-vcall 臂。

**拆箱（取出）**：取回到具体 struct 类型需拆回值 struct。`TypeChecker.StructUnboxTarget` 判「泛型返回
（get_Item / 方法返回 T）subst 后是否 blob struct」，是则调用点把结果包 `BoundConvert(→P)`，复用
`TypeOpEmitter._emitConvert` 的 `AsCast` 拆箱臂。`foreach (P p in list)` 在 `FunctionEmitter` 对元素发 `AsCast`。

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

`AccessEmitter` 谓词 `_isInlineStructFieldRoot`（字段类型 `IsBlobStruct` ∧ 容器是 class）+ `_isOwnerInlineField`
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
  - **泛型值 struct 数组的类型查找按擦除裸名**（fix-generic-value-struct-array）：`array_new` 携带的元素类型名
    是**非擦除全名**（`Kv<string, int>`，供 `arr.GetType().GetElementType()` 反射），但泛型是**类型擦除**——
    一个泛型定义只注册**裸名** `Kv` 的单一 `TypeDesc`。故 `try_struct_backed` 查类型前须 `element_type.split('<')`
    剥泛型实参、用裸名 `try_lookup_type`，否则用全名查 → miss → 数组退化成**引用背衬**（元素 `Null`）→
    `struct_fset_prim` 崩 `expected StructRef, got Null`（`KeyValuePair<K,V>[]` = `Dictionary.Entries()` 的返回，
    正是此路径）。全名仍传给 `struct_backed` 以保元素反射。一处修覆盖 interp/jit/数组字面量三条创建路径。
- **取值**：`array_get` 对 `StructBytes` backing 产 `StructRefHeap` 元素句柄（有 array `GcRef`，替代 `get_boxed`）。
- **codegen（AccessEmitter）**：`_emitArrayElemHandle`（ArrayGet 直发句柄不拷贝）；`_emitIndex` 对 struct[] 出
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
- ✅ **foreach over struct[]**（`foreach(P p in arr)`）：foreach 数组路径对值 struct 循环变量发的 AsCast，runtime
  `as_cast` 加 `StructRefHeap` 臂 → `copy_array_elem_out` 把元素拷出到当前帧 arena StructRef（值副本，循环变量非
  别名进数组）。runtime-only、格式中立，golden `struct_array.z42` foreach 段验（含 `foreach{e.x=999}` 不动数组）。
- ✅ **装箱引用身份 + struct 字段反射**（P4b add-boxed-struct-identity）：`BoxedStruct` 改共享 `ScriptObject`
  + `FieldInfo.GetValue/SetValue` 反射装箱 struct 字段（见下「装箱引用身份 + struct 字段反射」节）。
- ✅ **对象内联 struct 字段反射**（P4b-B add-object-inline-struct-reflection）：反射 `GetValue/SetValue` 读写
  `class C { Point pt; }` 的内联 struct 字段（复刻类级内联布局，见下同名节）。

## 基元装箱统一到 `BoxedStruct`（unify Phase 2 R3，2026-08-13）

**背景**：此前运行时有**两套不对称**装箱——struct 装箱 = `Value::BoxedStruct(GcRef<ScriptObject>)`
（GC 管理 + 引用身份）；基元装箱 = `Value::Boxed(Box<BoxedPrim{class, inner}>)`（轻量 `Box`、**无**引用
身份、**非** GC 管理）。代价：`is`/`as`/`GetType`/`value_to_str`/GC visit/equality/反射/vcall 等 ~20 处
helper 双写；且 `object o=5; object p=5; ReferenceEquals(o,p)` 在 C# 是 `false`（两个不同盒），旧基元装箱
无引用身份 → 语义偏差。

**统一（唯一装箱模型）**：`__box_prim`（`corelib/convert.rs`）改产堆 `ScriptObject` + `Value::BoxedStruct`，
与 struct 装箱同一路径 → 每次装箱 alloc 新盒（C# 引用身份），复用 `region_object` 全套 GC。删 `Value::Boxed`
变体 + `BoxedPrim` 结构（判别号 13 留空，`#[repr(C,u8)]` 显式判别 14-18 不重编、JIT 原始布局不受影响）。

**标量存储（D1-B：与 struct 装箱完全同构，零格式 bump）**：`__box_prim` 只装**整数**（bool/char/double/
string 各留自己的 `Value` variant，不经此路——proposal D5）。整数标量的 **LE 字节存进盒的 `struct_bytes`**
（宽度按 wrapper 名查 `well_known_names::int_wrapper_scalar_spec` → `Std.Int32`→4 / `Std.Byte`→1 /
`Std.Int64`→8…），`slots`/`struct_refs` 空。**关键**：基元 wrapper（`Std.Int32` 等）是 phantom struct
（零字段 / layout size 0），故装箱走**专用 alloc** `MagrGC::alloc_boxed_prim`（调用方按标量宽度定 `struct_bytes`
尺寸），**不**走 `type_desc.inline_regions()`（那会给零字段 wrapper 空 `struct_bytes`）→ wrapper 的 emitted
struct_layout / zbc TYPE section 完全不动，**无格式 bump**。

**拆箱**：`ScriptObject::boxed_prim_i64()`（`metadata/types.rs`）按 `type_desc.name` 的
`(width, signed)` 从 `struct_bytes` 前 `width` 字节还原 i64——signed narrow 符号扩展、unsigned 零扩展；
非整数盒（多字段 struct 装箱 / 名不在整数 wrapper 表）返 `None`。**这个 `Some`/`None` 是「基元盒 vs struct 盒」
的分流判据**：所有原 `Value::Boxed` 消费点收敛成单一 `BoxedStruct` 臂，需要区分时用 `boxed_prim_i64()`：

| 消费点 | 基元盒（`boxed_prim_i64()==Some(n)`） | struct 盒（`None`） |
|--------|-------------------------------------|---------------------|
| `is`/`as`（`exec_object` + `jit/helpers/object`） | 精确 wrapper 命中 → 拆回裸标量 `I64(n)` | 精确 struct 命中 → 拷 blob 回 arena `StructRef` |
| `(T)o` 数值转换（`convert_value`） | 拆回 `I64(n)` 再 convert | 不拦截（struct 不走数值转换） |
| VCall（`exec_vcall` + `jit/helpers/vcall`） | `this = I64(n)` 交基元 struct 方法体 | `this = 盒` 交合成对象协议方法 |
| `value_to_str` | 标量字符串（`WriteLine(object)` 打印 `5` 非 `Std.Int32{...}`） | `类型名{...}` 占位 |
| `GetType` | `type_desc.name` = 精确 wrapper（`Int64` 不丢宽度） | `type_desc.name` = struct 类型 |
| equality（`Value::eq`） | 装箱整数 vs 裸整数透明拆箱比较；盒 vs 盒按 `struct_bytes` | 盒 vs 盒按类型名 + `struct_bytes` + `struct_refs` |

**格式中立自证**：编译器发射不变（`__box_prim` 发射点 / 装箱路由不动）→ self-host 不动点逐字节复现
（gen1==gen2）；zbc/zpkg minor 不变。golden `types/boxed_primitive_is_as.z42`（Int64/Byte/Int32 跨宽度
is/as/GetType）+ `types/box_unbox.z42`（`(int)o` 拆箱 + `WriteLine` 装箱打印）验端到端。

### 为什么装箱必须带精确类型标记

`Value` 的内联 payload 只有 8 字节，塞不下「宽度 tag + i64」；而强类型的 `is` / `as` / `GetType`
要求装箱值**保留精确的基元类型**——`object x = 5; x is long` 必须为 **false**，`object l = 9L;
l is long` 必须为 **true**。裸 `Value::I64` 两者无从区分（未过 object 边界的裸整数走
`prim_isa` 松匹配，那是另一条路）。所以装箱一定要落到一个带 `type_desc` 的堆对象上，而不是
「codegen no-op / 直接把裸值塞进 object 槽」。

代价被限制在装箱点：算术与方法体永远拿拆箱后的标量，热路径零影响。收益是基元 wrapper 本身就是
真 struct（`struct Int32 : IComparable<int>`），带 type_desc 的盒经对象路径**免费获得**
is-a / `GetType` / vcall。

### 编译期：谁装箱、在哪装箱

装箱由 `TypeChecker.BoxIfNeeded(value, target)`（`TypeChecker.z42:196`）在每个协变点判定，命中则
包 `BoundBox`，codegen 由 `TypeOpEmitter._emitBox`（`:135`）降成
`const.str "Std.Int64"; builtin __box_prim %dst,%val,%cls`（`_emitBoxPrim`，`:163`）。
**复用既有 Builtin opcode，不新增 IR 指令 ⇒ 不 bump 格式。** 拆箱复用 `AsCast`：`BoxedStruct` →
基元时 is-a 校验后返还标量。

**哪些源类型真的装箱**（`BoxIfNeeded` 的分支序即判据）：

| 源静态类型 | 目标是 `object` / 接口时 | 说明 |
|---|---|---|
| **整数族**（`int`/`long`/`byte`/`short`/`uint`/… ）| ✅ `__box_prim`，`class` = 精确 wrapper | 标量 LE 字节进盒的 `struct_bytes` |
| **`enum`** | ✅ `__box_prim`，`class` = **enum 自身**（非 `Std.Int32`）| make-enum-distinct-type 1.5 |
| `bool` / `char` / `float` / `double` | ❌ 不装箱 | 各有自己的 `Value` 变体，自带身份 |
| `string` | ❌ 不装箱 | 引用类型 |
| **值 struct**（含泛型实例化 struct）| ✅ `__box_struct`，目标还包括**泛型形参** | 非 blob（单字段等）struct 的 `BoundBox` 在 codegen 退化为透传 |
| class / record / 数组 / 接口 | ❌ 恒等上转 | 本就是带 TypeDesc 的 GcRef |

> ⚠️ 「基元装箱」在 z42 里**只覆盖整数与 enum**。非整数标量不进盒这件事决定了：
> `((object)1.5).GetType()` 答 `Double` 不是靠盒，而是靠 `vcall_resolve` 阶梯第 3 级的
> `primitive_class_name`（见[对象协议派发](object-protocol-dispatch.md)）；
> 而 `ReferenceEquals` 式的盒身份只对整数 / enum / struct 成立。

**插入点**（协变点逐处插，缺一处就是一次静默丢类型）：

| 插入点 | 位置 |
|---|---|
| var-decl（`object o = 5L;`）| `StmtBinder.z42:257` |
| **再赋值**（`o = 5L;`，非声明）| `AssignTyper.z42:153` |
| return（返回类型 object/接口）| `StmtBinder.z42:227` |
| 数组字面量 `object[]` 的元素 | `ExprTyper._bindArrayInit`（`:311`）、集合字面量 `CollectionTyper.z42:63` |
| call-arg（形参 object/接口）| `TypeChecker.BoxArgs`（`:259-267`），由 `OverloadBinder._withDefaults`（`:243`）单点汇聚 |
| `params object[]` 尾包元素 | `OverloadBinder._withParamsExpansion`（`:385-399`）逐元素按**元素类型**装箱 |
| 索引器 set（`d[k] = v`）| `AssignTyper.z42:28-66` —— 手搭 `BoundCall`，**绕过 `BoxArgs`**，就地补装 |
| 泛型方法实参 | `ExprTyper.z42:164-203` —— 同上，手搭调用绕过 `BoxArgs`，逐位补装 |
| record 合成 `GetHashCode` 的字段 | `RecordSynth.z42:236` 直接发 `__box_prim` |

> 最后三行是同一个教训的三次复发：**任何手搭 `BoundCall` 而不经 `_withDefaults` 的路径都会漏装箱**。
> 「再赋值」那一处更直接——`BoxIfNeeded` 的头注释长期宣称覆盖「var-decl / 赋值」，而普通再赋值
> 其实**根本没有装箱点**，直到 `AssignTyper.z42:148` 补上。漏装箱的症状是安静的：裸标量流进
> `object` 槽，`is` / `GetType` 答错，或者裸 `StructRef` 句柄逃出创建帧后 use-after-free。

**拆箱消歧**：`(int)x` 有两义——① `x` 是 object / 接口 → 拆箱（`AsCast`）；② `x` 是数值 → 数值窄化
（`Convert`）。按 `x.Type()` 分派，绝大多数既有 cast 属 ②，不受影响。分类器口径见
reference 的[类型转换](../../../reference/src/language/conversions.md)。

**call-arg 与基元 native 的交互**：call-arg 装箱会把整数实参装成 object（`Assert.Equal(object,object)`
这类），而基元 struct 的 native 方法按裸 long 读参 —— `arg_i64`（`corelib/convert.rs` 取参助手）
**透明拆箱**基元盒，一处修覆盖全部整数 native。

### Deferred（装箱侧）

- **拆箱失败不可捕获**（`add-boxing-future-catchable-invalidcast`）：拆箱失败经运行期
  `Convert` / `AsCast` 的内部错误产生，当前是**终止性 VM 错误、不可 `try/catch`**——与所有
  `Convert` 失败一致。让它成为可捕获的 z42 异常是独立的既有问题，不属装箱机制。
- `add-boxing-future-enum-precise` **已完成**：enum 装箱现在带自己的 type_desc
  （`GetType().Name` 得 `Color`、`IsEnum` 为 `true`、`ToString()` 得成员名），不再塌成 `Int64`。

## JIT 值路径（add-struct-jit-value-path P5-A）

P5 前，JIT 一遇任一条 struct 值指令（`StructAlloc`/`StructCopy`/`StructFieldGetPrim`/`StructFieldSetPrim`）
即 `bail!`→**整函数回退 interp**——用到 struct 的函数拿不到任何 JIT 收益（连周边算术/循环/调用一起退回）。
P5-A 用 **helper 桥接**接通 JIT 值路径。

### 机制：helper 桥接（Decision D1 选 A，非原生内联）

每条 struct 指令 emit 成对一个 Rust helper 的 `call`（`jit_struct_alloc`/`_copy`/`_field_get_prim`/
`_field_set_prim`，`jit/helpers/struct_ops.rs`），helper 操作与 interp **同一个** per-context
`struct_arena`。**关键复用**：helper 只是薄封装读写 `JitFrame.regs`，真正的 arena 操作 + 字节编解码 +
base 三态分派（arena `StructRef` / 堆 `Object` 内联字段 / `StructRefHeap` 数组元素）全部调 interp
`exec_struct` 抽出的 frame 无关 `*_val` 核心（`struct_alloc_val`/`struct_copy_val`/`struct_field_get_val`/
`struct_field_set_val`）——interp 与 JIT **逐字节等价**，无逻辑分裂。

收益：**struct 指令本身 ≈interp 速度**（一次 native→Rust call + arena 锁），但**周边算术/控制流/调用为
native**——含 struct 的函数不再整体退回 interp。这是 P5 的 95% 价值。**原生内联字节访问**（FieldGet/Set
直接 emit cranelift load/store 到 arena 字节，跳过 helper call）边际提速有限却引入裸指针 × 移动 GC ×
realloc 健全性风险，记 **Deferred（P5-B）**——待 benchmark 证明某热路径卡在 helper 边界再做。

### frame_id：惰性分配 + OSR 继承

`StructRef{idx, frame_id}` 的 `frame_id` 供共享 arena 的悬垂 guard（LIFO base 已由现有
`push_frame`/`pop_frame` stamp `struct_base` 管理）。`JitFrame` 加 `frame_id: u32`（默认 `0`），采用
**纯惰性**——只在**分配型** helper（`jit_struct_alloc` / `jit_as_cast` 拆箱 / `copy_array_elem_out`）里，
若 `frame_id==0` 则从 `next_frame_id()`（与 interp 帧共用的单调 `AtomicU32`）取真值。deref（`FieldGet`/
`Copy`）用的是句柄里**内嵌**的 frame_id（非当前帧），故只有 alloc 路径需要——一处惰性覆盖入口 + 所有嵌套
callee，零 per-site 改动。**OSR 例外**：`from_interp_regs` 续接同一逻辑活动记录，须 eager **继承** interp
帧 frame_id（OSR 前已分配的 struct 局部交接后仍要能 deref）。

### 补齐：struct[] 数组 + 装箱拆箱

- **`jit_array_new`/`jit_array_new_lit`** 对 value-struct 元素造 `ArrayBacking::StructBytes` backing（复用
  interp `try_struct_backed`/`pack_struct_elem`），**`jit_array_get`** 对 StructBytes backing 产
  `StructRefHeap` 句柄（非 `get_boxed` BoxedStruct 快照）——接通 JIT 下 `new Point[]` + `arr[i].x` +
  `foreach(P p in arr)`（元素拷出走 `copy_array_elem_out`）。
- **`jit_as_cast`** 对 `BoxedStruct` 精确匹配**拆箱**到当前帧 arena `StructRef`（`(Point)o`），镜像 interp
  `unbox_struct`；`as object`·base·接口保持 boxed。

### 验证

golden `struct_jit.z42`（本地值语义 / 传参 sret / 嵌套 / string 叶子 / struct[] index+foreach / 装箱拆箱）
在 `--mode jit` EXIT=0，且与 interp 模式输出一致；既有 8 个 `struct*.z42` golden 在 `--mode jit` 全过（此前靠
bail→interp 才过，现真走 JIT struct 路径）。**格式中立、z42c 零改动（self-host 逐字节不变）。**

## 跨包 struct 值语义（add-crosspkg-struct-value-semantics P4a）

包 B `import` 包 A 定义的 struct 后按值语义工作（构造/字段/方法/传参 copy-in/返回/嵌套/值独立），与本地
struct 一致。

### 机制：跨包分类 imported struct（单点，复用既有 `HasBase` 编码，无格式 bump）

**wire 层面什么都不缺**：zpkg TYPE 段已携带 struct 标志（`Flags` bit2）+ 字段名/类型 + 完整字节布局
（`StructSize` + 引用位图），消费方 `ZbcReader` 也已解码进 `IrClassDesc`。缺的只是**编译器内部把「这是
struct」传过跨包这一跳**——`ImportedSymbolLoader` 造 imported `Z42ClassType` 时**从不设 `IsStruct`**（默认
false）→ imported struct 当引用类型。

**修复（单行）**：`nct.IsStruct = !cl.HasBase`。生产方 `ExportedTypeExtractor` 与消费方重建
`TsigReconcile._rebuildClass` 均把 struct-ness 编码进 `ExportedClassZ.HasBase`（`hasBase = !isStruct`——
非-struct class 恒 `HasBase=true`、struct 恒 `false`），故 `!cl.HasBase` **精确等价** isStruct（读同一份已
编码的权威 struct-ness，非启发式）。

**为何不加显式 `IsStruct` 字段**（bootstrap 约束，实测抓到）：`ExportedClassZ` 在 z42.ir（stdlib 库），
z42c.semantics 依赖它作跨包 API。给它加新 `IsStruct` 字段并在 z42c 源立即用 → 上一 nightly 种子的 z42.ir 无
此字段 → `xtask test bootstrap` 编当前 z42c 源报 `E0401: no field IsStruct`（bootstrap-seed axis ② stdlib
API 面越界）。复用既有 `HasBase` 零越界、一个 nightly 落地；若未来去 `HasBase` 重载，走两-nightly 迁移到显式
`IsStruct`。

分类正确后 `StructLayout.BuildFromSymbols` 从字段名/类型**重算**布局（`_compute` 确定性，与生产方持久化的
`StructSize`/引用位图**逐字节一致**）→ 发 `StructAlloc`/`StructFieldGetPrim/SetPrim`（正确字节 offset）。

**「逐字节一致」的第二个前提：字段类型拼写同口径（fix-crosspkg-nested-struct-layout, 2026-09-15）**。
`_kindOf` 按符号表**裸名键**判「字段是不是 struct」。本地字段拼写由 `MemberCollector` 取
`SurfaceTypeName(已解析类型)`（短名形式 `Point3`）；导入字段此前**照搬导出元数据的 FQ 串**
（`Demo.NestLayoutTarget.Point3`）→ 查不到 → 嵌套 struct 字段被判成 8B 引用叶子 → 消费方布局与生产方错位，
读写静默错值。修复：`ImportedSymbolLoader._fillClass` 登记 `OwnField` 时同样用 `SurfaceTypeName(fsym.FieldType)`
（解析失败才回落原串，与本地回落对称）。旧 fixture `struct_cross_pkg` 的 `Point{int,int}` 恰为 8B = 引用叶子
大小，偏移碰巧重合所以一直绿；golden `cross-zpkg/struct_nested_layout_cross_pkg` 用 12B `Point3` + transitive
`Frame` 守住（阴性对照：撤修复后 `Error: struct ref leaf at byte offset 8 not in type layout`）。

### 修复前的崩溃

`ImportedSymbolLoader` 从不设 `IsStruct` → imported struct 当**引用类型**（消费方不发 struct 指令、构造为
0 长 blob 的引用对象），而生产方 A 的构造函数按 struct 编译（发 `StructFieldSetPrim off=0`）→ 两包对「值
类型否」不一致 → 运行期 `struct field write out of blob bounds (off=0, w=4, len=0)`。golden
`cross-zpkg/struct_cross_pkg` 复现并守住修复（interp+jit 输出一致）。

## 装箱引用身份 + struct 字段反射（add-boxed-struct-identity P4b）

两件相扣的事，一个 change：**给装箱 struct 引用身份**（对齐 C#）+ **反射按字段名读写 struct 字段**。

### 装箱引用身份（路 B2：装箱进 `ScriptObject`）

PR2a 的 `Value::BoxedStruct(Box<BoxedStructData>)` 是**值**（`Box` 独占，`.cloned()` 深拷贝）→ `object b = a`
是两份独立盒、反射 `SetValue` 改盒调用方看不见，与 C#（box 是共享堆引用）不一致。P4b 把载荷改为
**`GcRef<ScriptObject>`**（共享堆句柄）：

- 装箱 = 分配一个 **struct 类型的 `ScriptObject`**（`type_desc.is_struct()`，struct blob 存进对象已有的
  `struct_bytes`/`struct_refs`，`slots` 空）。`inline_region_sizes()` 对 `is_struct()` 类型改读该类型自己的
  `struct_layout`（size + ref_count），使 `alloc_object` 为盒分配正确大小的 blob 区。装箱经
  `corelib::convert::box_struct_blob`（`__box_struct` 复用它），拆箱 `unbox_struct` 读对象 blob → 当前帧 arena。
- **复用 `region_object` + 全部 GC 机制，零 GC 核心改动**：GC 的 mark / gen-age / trace / scan_object_refs /
  size / 跨代写屏障（`maybe_mark_cross_gen_card` 的 owner+new 两侧）的 `BoxedStruct` 臂与 `Value::Object` **同路**
  （底层同为 `GcRef<ScriptObject>`）。`is/as/GetType/vcall/Equals` 保持 boxed 值类型特判，只改「读盒」经对象
  `type_desc.name`/`struct_bytes`/`struct_refs`。
- **收益**：`object b = a` 别名同盒、传参改盒可见、反射 `SetValue` 写穿——C# 引用身份达成。删 `BoxedStructData`。
- **不给 struct 加 base/vtable**（PR2a 决定不变）——只是把值装进已有对象容器、用 struct 自己的 TypeDesc。

### struct 字段反射（`FieldInfo.GetValue/SetValue`）

反射按**字段名**读值需 per-field 字节 offset + tag，但 runtime `StructTypeLayout` 只有 size + 无名引用位图。
解 = **Rust 复刻编译器 `StructLayout._compute`**（`corelib/struct_reflect.rs`，方案 B——格式中立、warm 本地可验，
非格式 bump 写偏移表）：

- 用 `TypeDesc.fields` 的 `(name, type_tag)` 逐字段自然对齐累积 offset，映射 `字段名 → (byte_off, tag, is_ref,
  is_struct, type_name)`。`canon`/`size_of`/`align_of`/`leaf_kind` 镜像 `Z42Type.Canon`/`_sizeOf`/`_alignOf`/
  `_kindOf`；`tag_from_name` 忠实镜像 `Tag.FromName`（decode signedness 与 codegen encode 一致）。嵌套 struct
  字段短名按声明类型的命名空间解析到 FQ（`resolve_named`）再递归。
- **三层校验**（`validate_against`，抓复刻漂移，不符即可 catch 的 `bail!`）：① `computed.size == 交付 size`；
  ② 计算的引用叶子 offset 集（含嵌套展平）逐一等交付 `ref_offsets`；③ 逐叶子 ref/prim 分类与交付位图交叉核对。
- **GetValue**：基元 → `decode_prim(struct_bytes)`；引用叶子 → `struct_refs[ref_index]`；嵌套 struct → boxed 快照
  （值语义，改返回盒不动父）。**SetValue**：共享盒**就地写穿**（引用身份→调用方可见）+ 引用叶子写屏障；基元实参
  透明拆箱（`object` 参数装箱的基元先 unbox 再 `encode_prim`）。
- 端到端 golden `reflection/struct_field`（GetValue 基元/string/嵌套 + SetValue 写穿+别名可见 + 值语义独立，
  interp+jit 双模式匹配 expected）+ `struct_reflect` 单元测试（布局/校验/tag signedness 护栏）。

### `field_get` 接受装箱 struct（accept-boxed-struct-field-get，2026-09-25）

值 struct 经**擦除的返回位**流出泛型函数时（`T id<T>(T a)` 的 `id(v)`），运行期的值就是上面那个
装箱 `ScriptObject`。此前**只有 `field_get` 一条指令不认它**，于是同一个接收者上
`id(v).Sum()` 正常、`id(v).X` 抛 `FieldGet: expected object, got BoxedStruct(…整屏堆转储…)`。

- **为什么落到通用 `field_get`**：调用点的静态类型是裸 `T`（`--dump-bound` 实测
  `(call id … :T)`、成员 `:<unknown>`）⇒ `AccessEmitter._emitMember` 的
  `_isBlobStruct(m.Target.Type())` 判假 ⇒ 不走 `struct_fget_prim`（那条**早就**认盒）。
- **修法 = 复用反射那条已验证的按名取叶子路径**（`accessors::boxed_struct_field_get`，
  含 `validate_against` 布局对账），interp（`exec_object.rs`）+ JIT（`jit_field_get`）**两条臂对称**
  —— 只补一侧的话小用例全绿而热代码崩（`jit_field_get` 的 `StackArray` 臂就这么漏过一次）。
- **`field_set` 刻意不跟着加**：写进「从擦除返回位流出的临时盒」必然被丢弃，正解是编译期拒绝
  （C# 同）⇒ 独立登记 `reject-assign-to-erased-call-result`。
- 端到端 golden `generics/erased_return_blob_field.z42`（四种叶子 + 显式/推断型参 + 静态方法承载
  + 200k 次热循环逼出 OSR 走 JIT 臂）。⭐ **两条臂各自的阴性对照都做过**：撤 interp 臂 → interp
  措辞红；撤 JIT 臂 → **JIT 措辞红**（证明热循环真的进了 `jit_field_get`，而不是全程解释执行）。
- ⚠️ 验这类修复必须 `xtask build runtime` **再** `build sdk`：`build sdk` 只装配、**不重编 Rust**，
  只跑后者会拿到上一轮的 `z42vm`，得到一字未变的假阴性（本刀的阴性对照第一次就这么假绿了）。
- 📉 **已知代价（不是回归 —— 这条路此前是崩）**：`struct_reflect::compute` + `validate_against`
  **每次访问都重算**（反射那条是冷路径，从来没人给它加缓存）。实测 interp 下 500 万次
  `id(v).X` = 2.29s，同规模普通对象字段读 = 0.78s ⇒ 每次访问约 300ns 的布局重算。
  登记 `cache-struct-reflect-layout`（按类型名缓存 `ComputedLayout`；布局按类型不变、缓存天然安全，
  反射侧同样受益）。本刀不做：缓存要挂在 `VmContext` 或 `TypeDesc` 上，属独立取舍。

### 对象内联 struct 字段反射（`class C { Point pt; }`，add-object-inline-struct-reflection P4b-B）

P4b 只交付**装箱 struct** 的字段反射；**堆对象上的内联 struct 字段**（`class C { Point pt; }`）此前反射
`GetValue(fi_pt, c)` 读的是 P3b 的 **dead slot → `Null`**。P4b-B 补齐读写路径，**复用同一套字节解码基础设施**：

- **类级内联布局复刻**（`struct_reflect::compute_class_inline`，镜像编译器 `StructLayout._computeInlineLayout`）：
  与 struct `_compute` 不同——类**只把 struct 字段**按声明序打包进对象 `struct_bytes`（自然对齐、引用叶子展平进
  `struct_refs`），**非 struct 字段仍在 slots**。产出「struct 字段名 → 对象相对 (byte_off, size, 引用叶子)」+
  对象相对引用位图，用 `validate_against` 对交付的 `TypeDesc.inline_layout`（P3b 已 wire 的合成布局）做同款三层
  校验抓漂移。
- **GetValue**：`struct_field_fq` 判定字段是否内联 struct——是则从 `struct_bytes`+`struct_refs` 物化 **boxed 快照**
  （值语义，改快照不动对象）；否则回落普通 slot 读。嵌套 struct 叶子（`Frame{Line edge}`）递归展平。读取逻辑与
  装箱 struct 的嵌套字段共用 `snapshot_struct_leaf`。
- **SetValue**：内联 struct 字段把传入的 boxed struct 字节+引用叶子**就地写穿对象共享字节区**（对象是堆节点 →
  引用身份可见，别名/后续读都见新值）+ 引用叶子写屏障。写入逻辑与装箱 struct 的嵌套字段共用 `write_struct_leaf`
  （该 helper 补齐了嵌套引用叶子的写屏障——根因修，装箱路径亦受益）。
- golden `reflection/struct_field` 扩：普通 slot 字段仍走 slot（`id`/`label`）+ 内联 struct 字段 GetValue 快照 +
  SetValue 写穿（对象直读验证）+ 快照值语义独立 + 嵌套内联（`Frame{Line edge}` 展平引用叶子）；`struct_reflect`
  加 3 个类级布局单测。**纯 runtime、格式中立、self-host 不受影响。**

> **Deferred（follow-up）**：反射 invoke boxed struct 合成方法（Equals/GetHashCode/ToString）、static struct 字段反射。

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
  （add-struct-method-return，`sret × VCall`）+ **foreach over struct[]**（add-struct-foreach，`as_cast`
  StructRefHeap 臂拷出元素）——均格式中立，golden `struct_array.z42` / `struct_heap_inline.z42`(GetPt) 验。
- ✅ **JIT 值路径**（helper 桥接，add-struct-jit-value-path P5-A）：struct 指令 emit 为 helper call 操作
  共享 arena、复用 interp `*_val` 核心，含 struct 的函数不再整体 bail→interp（见上「JIT 值路径」节）。
- ✅ **跨包 struct 值语义**（`import` 别包 struct，P4a add-crosspkg-struct-value-semantics）：`ImportedSymbolLoader`
  `nct.IsStruct = !cl.HasBase`（复用生产方 `HasBase=!isStruct` 编码，不新增 stdlib API→零 bootstrap 越界），
  消费方重算布局与生产方逐字节一致——修 imported struct 被当引用类型的 blob-bounds 崩，格式中立，golden
  `struct_cross_pkg`（含 transitive 嵌 imported struct）验。
- ✅ **装箱引用身份 + struct 字段反射**（P4b add-boxed-struct-identity）：`BoxedStruct` 载荷改共享
  `GcRef<ScriptObject>`（对齐 C# 引用身份，复用 region_object 零 GC 改动）+ `FieldInfo.GetValue/SetValue`
  反射装箱 struct 字段（Rust 复刻 `_compute` + 三层校验，格式中立）——见上「装箱引用身份 + struct 字段反射」节。
- ✅ **对象内联 struct 字段反射**（P4b-B add-object-inline-struct-reflection）：`FieldInfo.GetValue/SetValue`
  读写 `class C { Point pt; }` 的内联 struct 字段（复刻**类级**内联布局 `compute_class_inline` + 共用
  `snapshot_struct_leaf`/`write_struct_leaf`），格式中立——见上「对象内联 struct 字段反射」节。
- ✅ **泛型 struct 套 struct 的链式读写**（`t.Item1.Item2` / `pp.First.Y = v`）：链节须真内联才累加偏移，
  擦除成 `T` 的字段断链取句柄（fix-generic-struct-chain-access）——见上「嵌套 struct 字段」节。
- ✅ **泛型擦除槽的值复制**（generic-struct-erased-slot-value-copy）——**按实例化算布局 + 部分具体化**。
  修前：struct 值存进声明类型为 `T` 的字段（泛型 struct **与泛型 class** 都是）时按句柄存、**不复制**，
  复制外层也只浅拷句柄 ⇒ 与源变量/副本共享同一块 blob。三形态全错，其中泛型 class 那条还是
  **use-after-free**（栈帧作用域的 arena `StructRef` 存进 GC 堆对象字段槽，ctor 帧一弹 arena 截断 ⇒ 悬垂）。

  **根因不是「存入时忘了复制」，是那个槽物理上放不下字节**：`_kindOf("A")`（型参名）落
  `StructLeafKind.GcRef` 8 字节句柄，因为布局**按定义**算。故正解是让实例化拿到**自己的布局**
  （型参字段变真内联字节），并**按该布局特化其成员**——布局特化了，把偏移烘焙进指令的代码就必须跟着特化。

  **闸门** = `InstDiffersFromDef`（实例化布局确实不同于定义布局）。相同则共享定义那份体，
  特化是 no-op ⇒ 产出逐字节不变。**零格式 bump**（TYPE 段只是追加描述符条目）。

  **两个正交概念**（别混）：**身份名**管描述符 / `StructAlloc` / 数组元素名；**特化名**管布局查询 /
  成员派发。布局相同的实例化有独立身份但共享定义的成员体——共享体按定义偏移烘焙，布局既相同则对得上。

  > ⚠️ **「实例化是独立类型」的两个硬后果**：① 合成 `Equals` 的 `other is <类型>` 必须用实例化名，
  > 否则装箱值类型对不上 ⇒ **值相等静默误返 false**；② **VCall 按运行期类型名派发**，身份一变
  > `MyList<int>.Add` 就找不到 ⇒ 必须配**擦除名回落**（miss 后剥实参重试，只在 miss 路径跑）。

  **仍未覆盖**：① **普通泛型 class 不取独立身份**——给它独立身份要合成完整类描述符
  （基类链/接口/静态字段；**vtable 不需要** —— 运行期从 `own_methods` + 基链 merge），另开 change；
  ② **跨包实例化不特化**（消费方编译只读依赖签名，拿不到生产方方法体无法重发）⇒
  **`Std.ValueTuple` 在 z42.core，用户写 `(P2,int)` 是跨包，尚未覆盖**；
  ③ 容器 backing 仍靠 P3a 装箱（`List<T>` 内部 `new T[n]` 只编一份、元素名是字面 `"T"`）。

### 🔴 单调化必须是**闭包**（complete-generic-instantiation S1，2026-09-25）

#774 只做了**一半**：特化了实例化**类型**，没特化**操作它的泛型代码**。泛型体只编一份、按
**擦除布局**烘焙字节偏移，而调用方按实例化布局造值 ⇒ 同一批字节两种理解 ⇒ **静默错值**。

```z42
int ReadSecond<T>(Loc<T,int> p) { return p.Item2; }   // 体内：struct_fget_prim %0 @8（擦除布局）
Loc<P2,int> t = new Loc<P2,int>(a, 7);                // 实例化布局里 Item2 在 @16，@8 是 P2.Y
t.Item2           → 7   ✅
ReadSecond<P2>(t) → 2   ❌   （#774 起就错，虚派发形态读出 102 而非 107）
```

> **不变式**：凡是碰到 `G<A,B>` 的值的代码，都必须对它的布局达成一致。
> 部分单调化**按构造**违反它——这不是覆盖率问题，是正确性问题。

**根因一句话**：z42 的 IR 把**字节偏移烘焙进指令**，而泛型体只编一份。「一份体」与「多种布局」
必然对不上，除非每种布局各编一份体。

S1 把特化扩到：泛型自由函数 / 静态泛型方法 / 实例泛型方法（含虚与 override），与实例化类型的
特化**共用同一个不动点**——特化一个体会发现新实例化，特化一个实例化的成员又会调用新的泛型体。

两条只有实测才会知道的约束：

- **特化名不沿基类链继承**（`vcall_resolve`）。一份特化按**某一个声明**所在类型的布局烘焙了偏移；
  虚派发从接收者的**运行期**类型起走链，落到基类的特化体就是**静默调错实现**。
- **有类型错误时完全不做特化**。递归泛型（`Rec<T>` 调 `Rec<G<T>>`）让类型实参逐层加深 ⇒ 工作项
  无限增长 ⇒ **编译器不返回**。病态源码的 codegen 产物本就无意义，跳过让诊断正常输出；另有
  工作表上限兜底。

**跨编译单元（S1-f，已做）**：泛型声明与实例化在不同文件时，`SemanticModel` 由
`TypeChecker.Infer(cu, …)` **按 CU** 建，用当前 CU 的 `HasBody` 恒假 ⇒ 静默不发 ⇒
`MissingSymbolException: undefined function Demo.Loc<P2,int>.Loc`。修法是包级登记表
`GenericBodies` / `GenericTypeDecls` 里带上**声明所属 CU 的 model**（`GenericBodySrc.Model`）。

**仍未覆盖**：**跨包**实例化（S2）—— 消费方编译只读依赖的签名，拿不到生产方的方法体。

### 🔴 特化改变调用约定：sret 必须两侧同时翻（complete-generic-class-identity P4，2026-09-25）

泛型 **class** 的方法返回**类级型参**、而该型参被实例化成 blob 值 struct 时：

```
fn @Demo.G<P2>.Get(1) -> T {
  %1 = struct_alloc Demo.P2 [16B]   ← 在**自己帧**的 arena 里造
  …逐字段拷贝…
  ret %1                            ← 帧一弹即悬垂
}
```

⇒ `struct-value handle used after its creating frame exited — value-struct lifetime unsound`。

根因是**两侧都在看未代换的返回类型 `T`**：callee 的 `FunctionEmitter._blobStructNameT(T)` 判否
（`T` 既非 ClassType 也非 InstantiatedType），caller 的 `_isBlobStruct(c.Type())` 也判否
（装箱模型下 `c.Type()` 是 `Unknown`，外层包 `BoundConvert` 拆箱）。两边都判否 ⇒ **表面自洽**，
代价是返回一个已死帧的句柄。

> 🔬 **只修一侧＝换个地方错**：原型中只让 callee 走 sret，症状立刻变成
> `takes 2 physical argument(s), the call passes 1`。「lifetime unsound」与「签名对不上」
> 是同一条 bug 的两副面孔，取决于哪一侧先判出具体类型。

**修法（两侧 + 一道共同闸门）：**

| 侧 | 谁提供信息 | 做什么 |
|---|---|---|
| callee | `IrGenTypeEmitter.EmitInstantiation` 铺**类级** `SpecTypeArgs`（`T→P2`） | `_blobStructNameT` 认出代换后的 blob ⇒ 置 `METHOD_FLAG_SRET` |
| caller | typer 在 `MemberResolver` GS6 分支写 `BoundCall.InstRetType`（代换后的返回类型） | `CallEmitter._specSretName` 判定后预留返回槽、作末尾隐藏实参传入 |

⭐ **闸门必须是同一个**（design D5「判据单一出口」）：`_instLayoutName(receiver) != ""` ——
即「该实例化的成员**确实被重发过**」。它同时决定**成员派发名**与**调用约定**，不可能漂移成两把尺子。

⚠️ **调用约定是两侧协议，一侧单方面改就是 ABI 撕裂**：callee 侧的代换被
`SpecInstName != ""` 限定在**类实例化通道**。去掉这道闸门后，泛型**自由函数**的特化体
（`T makeValue<T>() where T : struct`）也跟着走 sret，而它的调用侧无从得知 ⇒
`Demo.makeValue:Pair … takes 1 physical argument(s), the call passes 0`（实测）。
泛型体那一侧今天两边都不代换 ⇒ 自洽（装箱模型），要改得连调用约定一起改。

> 为什么 caller 不自己算代换后的返回类型：那要**重做一遍重载决议**才能拿到被调方的声明返回
> 类型，查错就是错发 —— 同 `BoundCall.RetIsNullable` 的理由。typer 手里已有解析好的签名，
> 代换一次记下来即可；发射端只做它自己独有的那半判断（布局知识只在发射端）。

用例：`src/tests/generics/generic_class_returns_blob.z42`（六形态，含两条阴性对照）。

### 🔴 实例化成为运行期真正的类型（complete-generic-class-identity P1+P2，2026-09-25）

#774 让实例化有了**布局**，#820/#825 让操作它的代码跟着**特化**，但**声明形状**一直是擦除的：
普通泛型 class 的实例化根本不发描述符（闸门要求「有内联 struct 字段」），于是

| 形态 | 修前 | 应为 |
|---|---|---|
| `class DInt : GBox<int> {}` 的继承字段 | `null` | `0` |
| `o as GBox<string>`（o 是 `GBox<int>`） | 放行 → `VCall: expected object, got I64` | `null` |

两者**互相咬死**，不能只做一格：只给身份不改 `is`/`as`，`x is GBox<int>` 会从 true **静默**变 false。

**编译期（四处，共用一个出口）**

- `_instIdentityName` 的闸门取消 ⇒ 每个具体的**本包**实例化都有身份。
- `_instClassDesc` 发**完整**描述符：基类链 / 接口按实参代换，字段类型名代换。
  ⭐ **字段的集合与顺序必须从定义那条描述符 `_classDesc` 派生**，不能从 `StructLayout` 另起一份
  ——后者不含属性后备字段（`__prop_X`）等合成条目，两份对不上就是运行期字段槽错位，
  实测 `MulticastException<bool>.Results` 读出 `Null`。
- `fix-generic-base-name` 的剥名只对**开放**泛型基保留（`class Sub<T> : Bag<T>` 不是具体实例化，
  永远不会有描述符，原理由对它依然成立）；**闭合**基写实例化名并登记它。
- `is`/`as` 的目标名走 `_instIdentityName`（`BoundIsExpr.TargetType` / `BoundCast.Type()`），
  不再用丢实参的 `NamedType.Name`。

⚠️ **实例化名的基名必须 arity-mangled**（`Pair$2<int,string>`）。泛型类与同名非泛型类共存时
（add-class-arity-overloading），不 mangle 的话擦除前缀是 `Demo.Pair` —— 那是**非泛型**的那个类，
运行期擦除回落会调到它身上（实测 `p2.Describe()` 返回 "non-generic Pair"，**静默**错值）。

⚠️ 描述符投送必须**走到不动点**：造一条描述符会发现新的实例化（基表上的 `Bag<int>` 只有在造
`Sub<int>` 的描述符时才被登记）。快照一次 `Keys()` 就漏，实测
`base type Demo…Bag<int> of Demo…SubBag<int> could not be resolved`。

**运行期（三处，全是「擦除名是回落、不是身份」的贯彻）**

- `vcall_resolve`：擦除名回落要在基链的**每一层**做，不只接收者自己那层 ——
  `class DInt : GBox<int> {}` 的基是实例化，而成员体只以擦除名存在
  （实测 `VCall: function Demo.DInt.Tag not found`）。顺带修正了顺序：接收者自己的擦除定义
  现在先于**基类**的同名方法，此前的表后置放反了。
- `is_subclass_or_eq_td`：`x is GBox`（不带实参，C# 写不出来）意为「任何 GBox 的实例化」，
  故每层都比一次擦除前缀。两种拼写都要认：裸名 `Demo.GBox`，以及 arity-mangled 的
  `Demo.GBox$1`（**导入**泛型在元数据里的拼写，`StmtEmitter` 的 catch_type 就是它）——
  漏掉后者时 `catch (MulticastException<bool>)` 抓不住 `Std.MulticastException<bool>`。
- `build_type_registry`：**实例化的名字就是它的类型实参**，在这里解析成 `type_args`。
  编译期的 `ObjNew` 一旦用身份名就不再另发一份实参列表（发两份会渲染成 `Demo.Box<int><int>`），
  反射 `Type.GetGenericArguments()` 与泛型字段零初始化都读 `type_args` ⇒ 名字成为唯一真相。
  interp 与 JIT 的 `ObjNew` 必须**同时**回落到它（这一对曾经一边倒，正是
  `GBox<int>().V == 0` 两个引擎答案不一致的根因）。

用例：`src/tests/generics/generic_class_identity.z42`（B/D 两格 + **五条阴性对照**：
擦除名 `is GBox`、接口代换、派生类两个方向、开放泛型基、反射仍视其为构造泛型类型）。

### 🔴 静态成员按闭合类型各一份（complete-generic-class-identity P3 + P5-b，2026-09-25）

C# 里 `GBox<int>.Count` 与 `GBox<string>.Count` 是**两个槽**。z42 修前两侧都按擦除名拼键
——自洽，但语义错：各构造 2 / 3 次，两边都读出 5（实测）。

**三件事必须同时成立，缺一格就是另一种错：**

| | 做什么 | 缺了会怎样 |
|---|---|---|
| ① 键 | `AccessEmitter._staticKey` 是**唯一出口**（读 / 写 / 属性后备三条路都调它） | 同一个槽因走哪条路而拼出两个名字 |
| ② 体 | 成员按实例化各发一份 | **一份共享的体只能写一个键** —— 键改了也没用 |
| ③ 初始化 | 类型初始化器也各一份，描述符挂各自的 `$Cctor` | `static int Seed = 7;` 恒读出 0（定义那份 cctor 写擦除键、读方查实例化键） |

⭐ **②的闸门是 `IrGen.InstNeedsOwnBody`，发射侧与派发侧必须共用它**：

```
InstNeedsOwnBody(inst) = Layouts.InstDiffersFromDef(inst)      // ① 布局不同（#774 起）
                       || DefHasStaticState(基名)               // ② 定义有静态状态（本档）
```

两边错开的后果是确定的：只放宽**发射** ⇒ 特化体发出来但没人调（死代码，行为一字不变）；
只放宽**派发** ⇒ 调用点指向一个没发射的名字（运行期 MissingSymbol）。

⚠️ **`EmitStaticInit` 自己拼键、不经 `_staticKey`**，所以要在那里也认一次 `SpecInstName` ——
漏掉时 `Demo.GBox<int>.$cctor` 的体里写的还是 `Demo.GBox.Seed`（实测，症状就是 ③）。
这是本线第 N 次「同一判据散在多处」，只不过这次是**同一个键的两个拼法**。

**P5-b（解析器）**：`GBox<int>.Count` 此前 `E0202`。`<类型列表>` 后紧跟 `.` ⇒ 左边是**类型引用**，
无歧义（二元 `<` 的 `>` 之后不可能紧跟 `.`，那样缺操作数）。实参挂到左边的 `IdentExpr`
（`IdentExpr.TypeArgs`）而**不新造 AST 节点** —— 每加一个节点类型，每个 walker 都要补分支，
漏一个就是静默跳过。

📐 **实测：全仓 139 个泛型类型声明，带静态字段的是 0 个**（stdlib / 编译器 / 工具链 / 测试 /
示例全扫过）。两条推论：**不需要分阶段引入**（「生产方与消费方必须同代编译器」那个危险在自举
路径上没有任何实例），而 **CI 的 fingerprint 守门对这一档是瞎的**（stdlib 产物一字不变）⇒ 手动 bump。

用例：`src/tests/generics/generic_static_per_instantiation.z42`（三格 + **三条阴性对照**：
非泛型静态字段、`a < b` 仍是比较、实例字段仍每对象一份）。
- ⏳ Deferred：**单标量叶子 struct 塌缩**（`GCHandle`=Phase B）、**JIT 原生内联字节访问**（P5-B，现 helper
  桥接=interp 速度）、**反射合成方法可见**、**static struct 字段反射**、**ToString 字段 dump**、**E0438
  自引用诊断**（现 `Size==0` 兜底防崩）。
