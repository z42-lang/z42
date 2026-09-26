# 缺符号不再静默：用到才抛可 catch 的类型化异常

> 对应 change：`fix-silent-symbol-resolution`（2026-09-13）。
> 显式降级通道见[加载期可用性折叠与死分支剪枝](availability-folding.md)。

## 问题：一个哨兵编码了两件事

依赖 zpkg 的**版本 skew**（编译时依赖 v2、运行时加载到 v1）在 z42 里此前**几乎全是静默
错误答案，不是报错**。根因是单一的：`UNRESOLVED` 这个哨兵在设计上**同时编码**

- 「跨包待解析」——按需加载下，符号现在不在注册表里，下一个 zpkg 到场就有了；
- 「根本不存在」——依赖包真的没有这个符号。

所有下游兜底都按前者处理：一路 fallback、合成描述符、返 `Null`。于是

| 场景 | 修复前 | 修复后 |
|------|--------|--------|
| 读缺失的静态字段 | 静默 `Value::Null` | `MissingSymbolException` |
| `new` 一个解析不到的类型 | 合成零字段零 vtable 空壳 | `MissingSymbolException` |
| 构造器缺失（编译期见过它，或带实参） | 照常把**未经构造**的对象写进 dst | `MissingSymbolException` |
| 构造器 / 实例方法解析到了**另一个签名**（primary 裸键撞上） | 缺的形参停在默认值继续跑 | `MissingSymbolException` |
| 基类解析不到 | 子类静默退化成「只有自己的成员」 | `MissingSymbolException` |
| 静态调用缺失（interp） | 不可 catch 的 VM abort | `MissingSymbolException` |
| 静态调用缺失（JIT） | 裸 `Value::Str`，只能被无类型 `catch {}` 抓到 | `MissingSymbolException` |

## 判定原则：只在「确定不存在」时抛

把这条判错，会把**正常的跨包两阶段加载**变成崩溃。所以判据不是「现在查不到」，而是
**所有惰性解析路径都穷尽之后仍然查不到**。实现集中在
`src/runtime/src/vm_context/symres.rs` 一个模块里，两后端共用同一份判定，避免
interp / JIT 语义漂移（这是本仓最易出错的一维）。

三条反复出现的纪律：

1. **不能靠值判断。** 静态字段读不到返回 `Null`，而 `Null` 本身是合法值（未赋值的引用型
   字段就是 `Null`）。判据只能是**元数据**：该成员是否真的声明在属主类型（或其基类）上。
2. **证不出来就放行。** 基类链中途解析不出来 ⇒ 无法证明「没声明」⇒ 保守放行。宁可漏掉
   一些，也不要把正常加载打崩。
3. **但保守要有边界。** 「解析路径没走完就别下结论」是保守；「凡是拿不到就别报」不是。
   `try_lookup_type` **本身就是完整解析路径**（会触发所属包加载），它失败就再无回落。
   站点 ① 最初误把「属主类型解析不出来」当成别人的事跳过，结果对**最常见的 skew 形态**
   （整个依赖包不在了）完全不生效——实测才发现。

## 各站点的判据

### 静态字段 —— `verify_static_field`

只在**读到 `Null` 时**才做校验（非 `Null` 读零成本）。裁决是三态，不是两态：

| 裁决 | 条件 | 动作 |
|------|------|------|
| `Ok` | 字段已声明且是引用型，或基类链没走通 | 照常返回 `Null` |
| `Default(v)` | 字段已声明且是**值类型**，槽位却停在 `Null` | 按声明类型补零值，并**回写槽位** |
| `Missing(exc)` | 属主类型解析不出来，或字段不在声明里 | 抛 |

`Default` 这一支是顺带修掉的另一个静默错误：值类型静态字段无初始化器时槽位停在 `Null`
（`resize_with(|| Value::Null)` 只填 `Null`，没人按声明类型零初始化），`static int N;` 一读
就崩在 `__box_prim: expected integer value, got Null`。回写让后续读不再走这条路。

### 构造器 —— `missing_ctor_exception`，判据是编译期的**正向位**

这条最反直觉：**不能用「ctor 名解析不到」当判据**。z42c 对**没有构造器**的类照样发射
`ObjNew`，ctor 键取裸类名（`Demo.Point.Point`）——而这与**单构造器**的 primary 裸键
（见 `stabilize-instance-dispatch-keys`）**同形**。运行时无法从名字本身区分「这个类没有
构造器」和「构造器应该在但不见了」。`IrLoopAllocReuse` 的裸分配（ctor 名为空串）也走
同一条路。

最初（`fix-silent-symbol-resolution`）只能退而求其次，拿「有没有实参」近似：没有构造器的
类不可能接受实参，故 `argc > 0` 且全路径解析不到必然是缺失。代价是 **`argc == 0` 是一条
公开的缝**——最常见的 `new C()` 恰恰落在缝里。

`encode-ctorless-objnew`（zbc 1.39）把这条缝补上：**让编译器把它知道的事写进指令**。

#### `ctor_known`：为什么是正向位

`ObjNew` 尾部多一个 `ctor_known:u8`。编译器在**整包装配之后**（`CtorKnownFixup`）检查该
ctor 名是否出现在「本包全部已发射函数 ∪ `DependencyIndex`」里 —— 出现才置 1。于是：

```
ctor_name 为空          ⇒  裸分配，从来不指名任何构造器，放行
ctor_known 且解析不到   ⇒  编译期确实看见过它 ⇒ 定案缺失，抛
argc > 0  且解析不到    ⇒  零构造器的类不可能收实参 ⇒ 定案缺失，抛（与上条取并集）
其余                    ⇒  证不出来，照旧走「无 ctor」路径
```

**方向很重要。** 反过来编码（「零构造器」发空 ctor 名）看着更省事、还能复用
`IrLoopAllocReuse` 现成的约定，但它会**新引入**一种静默：编译时依赖 v2 的 `class C { }`
发空名，运行时装到 v1 的 `class C { C() {…} }` ⇒ v1 的构造器被悄悄跳过。正向位没有这个
问题 —— **位的缺席是保守态**，证不出来就不置位、名字原样保留，运行期行为与 1.38 逐字一致。
这是「[判定原则](#判定原则只在确定不存在时抛)」第 2 条在编码层面的体现。

#### 为什么判据必须等到整包装配之后

发射端（`CallEmitter._emitNew`）那一刻答案还不存在，有两个独立原因：

1. z42c 会为「有字段初始化器、无显式 ctor」的类**合成**隐式构造器
   （`IrGenTypeEmitter._emitSynthCtor`）。它既不是 `MethodSymbol`（符号表判据会把它误判成
   零构造器），也不在 `_bindNew` 那一刻存在 —— `DeclBinder._synthCtors` 跑在
   `TypeChecker.Infer` 的**全部绑定之后**。
2. 本 CU 的 `IrModule` 看不到**同包其它文件**的函数，而 `DependencyIndex` 按设计不含本包。

装配点（`PackageCompile` ⑩）两者都齐了：所有文件的 IR 都已发射完毕，合成构造器就是其中一个
普通的已发射函数，`DependencyIndex` 也在手。这也是为什么这个 pass 不能做成 per-module 的
IR pass 而必须挂在装配上。

**增量安全**：fixup 每次装配都**重算**全部站点（不是只置位、不是 OR）。增量编译里从缓存
复用的 `IrModule` 同样在重扫范围内，所以「A 文件缓存着旧结论、B 文件刚给那个类加/删了构造器」
不会留下过期的位。

### 签名对不上 —— `call_arity` + `wrong_arity_exception`，解析**成功**也要查

上面那条判据只管「解析不到」。但 **primary 裸键**（声明序第一个同名成员用裸名，见
`stabilize-instance-dispatch-keys`）在版本 skew 下还会**命中错的签名**：

| | 编译时依赖（v2） | 运行时加载到（v1） | 调用点发出的键 |
|---|---|---|---|
| 构造器 | `Widget()` | `Widget(int v)` | `Demo.T.Widget.Widget`（两边都是） |
| 实例方法 | `string Label()` | `string Label(string prefix)` | `Demo.T.Box.Label`（两边都是） |

键**解析成功**，命中的却是另一个签名。建帧（`Frame::new` / `new_from_regs` /
`new_from_receiver_regs`）**不做任何 arity 校验**，缺的形参就停在默认值上继续跑——实测
`new Widget()` 把字段写成 `0`、`b.Label()` 输出 `null7`。这不是「缺符号」，是**静默调错**。

**覆盖面**：构造器（fix-ctor-arity-skew）、实例方法——`VCall` 与 sealed 去虚化后的直接 `Call`
（fix-call-arity-skew）、静态虚成员。**常规静态方法天然免疫**：它们的键恒为全签名 mangle
（`OverloadResolver.MangleKey`），签名一变键就变、解析失败，由「缺符号」那条路报——
`src/tests/cross-zpkg/call_arity_static_skew` 守住这个事实。

#### 判据：精确相等，下界不读 `min_arg`、上界读 sret 位

```
phys  = 调用点实际传入的值个数（含 this、含 sret 槽）
want  = param_count + (method_flags & METHOD_FLAG_SRET ? 1 : 0)
params 变长 ⇒ phys ≥ want；否则 phys == want
```

两条都来自在全量 `xtask test` 上给解释器的三个函数体入口挂探针的**普查**，不是推断：

- **合法调用里「实参数 < 形参数」0 次。** z42 的默认值由**调用点**在编译期填满（跨包构造器那一支
  由 #623 补齐）⇒ 任何少于 `param_count` 的调用都是 skew，典型是「被调方新加了一个可选参数」。
  构造器此前用 `min_arg` 当下界，恰恰放过了这种 skew；那段为 `min_arg` 两种口径并存而写的夹取补丁
  随之整体删除。
- **「实参数 = 形参数 + 1」有 10 个合法站点**，全是返回 blob 值 struct 的函数：caller 在末尾传一个
  **sret 隐藏返回槽**，`FunctionEmitter` 为了不污染反射/跨包签名，故意**不**把它计入 `param_count`。

所以 sret 是**物理签名的一部分，却从没写进元数据**——运行时拿到的 `param_count` 是一个少报了一的数。
可选的判据都是在猜：

| 方案 | 为什么没选 |
|---|---|
| 容一（`param_count ≤ phys ≤ param_count + 1`） | 「被调方恰好少一个参数」与 sret 永远分不开——设计出来的洞 |
| 运行时认 blob struct（看 `ret_type`） | 在 VM 里复刻编译器 `_isBlobStruct` ⇒ 同一规则两份实现，漂移方向是**误杀合法调用** |
| 从 IR 形状推断（非 void 返回但 `Ret` 都不带值） | 从函数体反推调用约定；例如只抛异常、没有 `Ret` 的函数会被误认成 sret |

**定案**：`method_flags` 新增 **bit3 = `METHOD_FLAG_SRET`**（zbc 1.40），由 `FunctionEmitter` 在
`RetIsStruct` 时置位（全仓唯一决定 sret 约定的地方），各 `IrGen*Emitter` 与修饰符位**按位或**合并。
给既有字段加位语义也必须 bump：否则旧产物该位恒 0，新 VM 会把所有「返回 struct 的旧调用」判成错签名。

#### 在哪里查：只在「首次绑定」处，热路径零开销

| 路径 | 首次绑定点 | 命中缓存后 |
|---|---|---|
| 合并模块内 `Call`（**含急切合并的 `z42.core`**） | resolver Pass 2 预填 `method_tokens`：对不上就**不预填** | token 直取 |
| 同上，未预填的站点 | `exec_call::call` 未命中后写回 token 之前 | — |
| 跨包 `Call`（interp） | 填 `cross_module_targets` 的 `OnceLock` 之前 | 借用 cell |
| 跨包 `Call`（JIT） | tier 3 写 `call_jit_ic` 之前（按名取 `Function` 判，**不用** `FnEntry`：被调方未到 JIT 阈值时拿不到它，IC 却照写） | IC 直取 |
| `VCall`（两后端共用 `resolve_vcall`） | 出口统一判 → `VCallTarget::Thrown`；`install_ic` 对不上**不装 PIC** | PIC 直取 |
| `ObjNew` | 解析到构造器后（interp / JIT 各 native 与惰性分支） | — |

两处容易漏的地方，都是真实推导出来的：

- **resolver 预填必须拦**。`z42.core` 被急切加载并**合并进主模块**，用户代码调 stdlib 走的是加载期预填的
  模块内下标——只查跨包分支会漏掉**对 stdlib 的 skew**，而那恰是最常见的形态。JIT tier 1 读的就是这组
  token，所以拦一处两个后端都退到冷路径。
- **缓存写入必须晚于判定**。被拒的绑定若进了 PIC / cell / IC，下一次命中缓存就直接派发、再不经过判定。

复用 `MissingSymbolException` 而不新增异常类：新类要先进 stdlib，而冷启动种子的 stdlib 里没有它
⇒ 得走两-nightly。语义上也说得通：调用点指名的那个签名**确实不在**，撞上的是同键下的另一个。

> ⚠️ **判定也会抓到「根本不是 skew」的调用。** 上线第一轮就在 z42b 里拦下一条**同包**调用：
> `_pubBundleProjectDeps` 要 4 个参数（末参无默认值），调用点只传了 3 个——源码写错，参数一直静默为 `Null`。
> 根因是 **z42c 对普通调用「实参少于必填形参」不报错**（构造器有 E0426，方法/自由函数没有，同文件也放行），
> 于是编译器发出了一条参数不足的 `Call`。运行期判定是它的兜底；编译期诊断另行补齐。
>
> 普查的覆盖边界也由此可见：探针挂在解释器的函数体入口，JIT native 直调不经过——那一处正走 native。
> 普查只用来定「合法调用长什么样」，判定本身挂在两后端共用的绑定点，不依赖探针覆盖。

#### 谁负责传 sret：裸名入口一律不传（unify-blob-return-abi，2026-09-26）

上面那条判定是对的，但它把一条**编译期约定的自相矛盾**暴露成了运行期异常：

- sret 由**调用点的静态返回类型**决定（`CallEmitter` 的 `_isBlobStruct(c.Type())`）；
- 而它是**每方法固定**的 `method_flags bit3`；
- 且 VCall **只按方法名**索引 vtable 槽（arity 不入解析键）。

⇒ **凡「调用点看不见具体返回类型」的派发边界，两侧必然对不上。** 已知三副面孔，同一个根因：

| 形态 | 调用点物理实参 | callee | 措辞 |
|---|---|---|---|
| 接口声明返回**引用型**、实现返回 blob struct | `this` = 1 | `this`+sret = 2 | `takes 2 …, passes 1` |
| `static abstract Self op_Add(Self,Self)` 在**擦除的泛型体**里 | `this`+1 = 2 | 2 形参+sret = 3 | `takes 3 …, passes 2` |
| 实例 `Self Copy()` 经**接口收者** | 1 | `this`+sret = 2 | `takes 2 …, passes 1` |

**约定收口**：返回 blob struct 的方法，只要它**可能被裸名派发**，就让
**桥接占裸名槽**（无 sret，`__box_struct` 后返回引用），具体实现挪到 `<m>$struct`（带 sret）；
静态可解析的直接调用点经 `MethodSymbol.CallKey()` 绑到后者，**无装箱快路径零开销、字节不变**。

判据在**符号层**（`InheritanceResolver` 打 `IfaceBridgeRet`），且必须看**未代换**的接口声明
（`ims.Signature.Ret is Z42GenericParamType`）—— `Self` 经满足性检查已被换成实现类（= 那个 struct），
单看代换后的类型分不出「声明写的是 `Self`」（要桥接）与「声明写的就是这个具体 struct」
（调用点**知道**要传 sret，桥接反而会打坏）。后者由 golden `interfaces/self_return_blob_struct.z42`
的 `IExact` 那一组守着。

#### 三条「绕过 VCall 的捷径」里，静态那条漏了 sret（fix-crosspkg-static-sret，2026-09-26）

`CallEmitter._emitCall` 有三条捷径，**两条实例路一直正确拼 sret 槽，静态那条从一开始就没拼**：

| 捷径 | sret |
|---|---|
| devirt（sealed / 精确类） | ✅ |
| DepIndex **instance** | ✅ |
| **DepIndex `static`** ＋ **静态属性访问器的依赖分支** | 🔴 修前直接发裸 `CallInstr` |

⇒ 跨包调用一个返回 blob 值 struct 的**静态**方法/属性：生产方按 sret 编、消费方少传一槽
⇒ 编译期零诊断、运行期 `takes N+1 physical argument(s), the call passes N`。

**为什么四个月没响**：全仓没有任何 golden 跨包调用过「返回 struct 的方法」——
`struct_cross_pkg` 只测跨包**构造**与**字段读**，而 `z42.core` 里**一个多字段 struct 都没有**
（`GCHandle`/`Guid` 各 1 字段 + 12 个零字段基元 wrapper）⇒ 这条路在 stdlib 上走不到。
守门的 fixture 现在有了：`src/tests/cross-zpkg/single_field_struct_cross_pkg/`。

⚠️ **调查工具的陷阱**：`z42c --dump-ir` / `--dump-bound` **不加载 stdlib/依赖**（带 `Z42_LIBS`
也一样）⇒ 用它们看「跨包调用点发了什么」会得到假象（我据此错判成 loose VCall）。
可靠办法：在编译器里打点，或只信运行期措辞。

⚠️ **为什么不让 VM 按目标 flags 自适应**：那会把每方法固定的 ABI 变成**派发时协商**，
与本页判定「精确相等」的立场反向，且 JIT 要发条件化调用序列 ⇒ 接口/泛型调用整体降级回解释执行。
`add-iface-return-bridge` 的 D1 已裁决过同一个问题。

### 类型 —— `missing_type_exception`

回落描述符（`make_fallback_type_desc`）有一个**正当用途**：合并进来的 stdlib 模块不带预建
`TypeDesc`，但带 `ClassDesc`——按 `module.classes` 的继承链现建一个，字段槽是齐的。判据
就是这条链在不在：

```
module.classes 里有            → 回落描述符正确，放行
没有，且名字不带点            → 编译器合成的本地类（闭包类等），放行
没有，且名字带点              → 跨包引用没解析到 ⇒ 定案缺失，抛
```

这三条此前就写在一条 `tracing::warn!` 的判断里（`defer-class-initialization`）。日志挡不住
静默数据损坏：空壳没有字段槽 ⇒ 构造器的 `FieldSet` 被**丢弃**、后续 `FieldGet` 全读
`Null`（实测：`Std.IO.Process` 被合成空壳后，崩在 `AppendString` 的 `arr.Length`）。判据
既然已经确定，就该抛。

### 基类 —— `base_unmerged` 旗子 + `missing_base_exception`

四个站点里最危险的一个：丢的不是一个符号，是基类的**全部字段槽和 vtable 条目**。

`TypeDescCold.base_unmerged` 标出「`base_name` 有声明，但构建本描述符时那个基类不在当时的
注册表里」⇒ `fields` / `vtable` 退化成「只有自己的」。

- **置位**：`build_type_registry`，`desc.base_class.is_some() && !registry.contains_key(b)`。
- **清位**：`try_fixup_inheritance`。⚠️ 清位**不能挂在 `needs_fixup` 上**——基类一旦可解析，
  `needs_fixup` 比的是「字段 / vtable 条数对不对得上」，基类若**零字段零非静态方法**（标记
  基类）条数天然相等 ⇒ `needs_fixup` 恒 `false` ⇒ 旗子永远清不掉 ⇒ 把一个其实完好的类型
  误判成「基类缺失」。故单独收一份「基类已可解析」的清位名单。清位本身**不计入**
  `newly_fixed`（那是不动点循环的终止条件）。
- **冷区保活**：`class Empty : CrossPkgBase {}` 的冷区可能什么都不剩，丢了冷区就丢了旗子，
  故冷区丢弃条件里加 `&& !cold_inner.base_unmerged`。

`ObjNew` 见到旗子先**去惰性加载器取一份**（`try_lookup_type` 内部会
`ensure_base_chain_loaded` + 把继承 fixup 跑到不动点）；取回来**仍然**带旗子，才是站点 ④
的定案。

## 顺带修掉的两个既有 bug

这两个都不是 skew 问题，是被静默行为盖住的真 bug——**一个静默失败的特性会掩盖别的 bug**
在这个 change 里又应验了两次。

### 主模块的类继承跨包基类时丢掉全部继承字段

`class Derived : CrossPkgBase` 写在**主模块**里时，`d.A = 7` 被静默丢弃、`d.A` 读出 `Null`
（实测急切副本 1 个字段槽 / 惰性副本 2 个）。**基类在不在场都一样**，与 skew 无关。

根因是两份注册表的分叉：主模块的类型注册表在**构建期**合并继承视图，那时跨包依赖还没
加载 ⇒ 退化成「只有自己的字段」；`try_fixup_inheritance` 事后能补齐，但
`fix-projecthooks-vtable-fixup` 的 `Arc::make_mut` 写时复制只让**惰性**注册表拿到修好的
副本。而 `ObjNew` **先查主模块** —— 于是永远拿残缺的那份。

`VCall` 另有基类回落路径（`fix-vcall-base-class-fallback`），所以 `d.Who()` 一直是好的，
把这个洞盖了很久。修法即上面的 `base_unmerged` 旗子 + `ObjNew` 改取修好的那份。

> **教训**：方法派发通过 ≠ 字段布局正确。跨包继承的用例两样都要测。

### JIT 的回落描述符是空的

`ObjNew` 的 JIT 助手此前**就地合成一个空 `TypeDesc`**，而 interp 走
`make_fallback_type_desc`（按 `ClassDesc` 继承链把字段槽建齐）——同一个「合并模块不带预建
`TypeDesc`」的合法回落，JIT 下却丢掉全部字段。现已改为两后端共用同一份实现。

## 想主动降级怎么办

用编译期宏 [`available!(X)`](../../../reference/src/language/available-macro.md) 把那条分支保护起来：被保护的
分支会在**加载期整块剪掉**，其符号永不参与解析，因此不会触发本异常。这是本机制的
**唯一显式豁免通道**。

```z42
if (available!(NewApi.Feature)) {
    Console.WriteLine(NewApi.Feature().ToString());
} else {
    Console.WriteLine("legacy-path");   // 旧依赖下走这里，不抛
}
```

## 测试脚手架

skew 场景靠 `src/tests/cross-zpkg/` 的两个可选标记文件，都在 run 波之前生效，且**必须同时
改两处**——临时 `Z42_LIBS` 与 `main/<dist>`（packed exe build 会把依赖 zpkg colocate 进
main dist，而惰性加载器**先搜 entry zpkg 同目录**，只改 libs 那份等于没改）。

| 标记文件 | 语义 | 演示 |
|----------|------|------|
| `skew-absent.txt` | 运行前**删掉**指定 zpkg | 依赖包整个不在场 |
| `skew-replace.txt` | 运行前用 `oldtarget/` 的同名产物**顶替** | 依赖包在场但**成员更少**——skew 的常态 |

`oldtarget/` 是与 `target/` **同工程名**的旧版依赖工程，**不参与 `main` 的编译**：main 仍按
新版编译，运行时却加载到旧版。每个抛异常的用例都配一个**基类/构造器在场**的对照组，
防止判据被放宽后无人察觉。
