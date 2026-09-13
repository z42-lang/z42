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
| 构造器缺失（带实参） | 照常把**未经构造**的对象写进 dst | `MissingSymbolException` |
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

### 构造器 —— `missing_ctor_exception`，判据是「有没有实参」

这条最反直觉：**不能用「ctor 名解析不到」当判据**。z42c 对**没有构造器**的类照样发射
`ObjNew`，ctor 键取裸类名（`Demo.Point.Point`）——而这与**单构造器**的 primary 裸键
（见 `stabilize-instance-dispatch-keys`）**同形**。运行时无法从名字本身区分「这个类没有
构造器」和「构造器应该在但不见了」。`IrLoopAllocReuse` 的裸分配（ctor 名为空串）也走
同一条路。

唯一可证的事实是：**没有构造器的类不可能接受实参**。故

```
argc > 0 且全路径解析不到  ⇒  定案缺失，抛
argc == 0                  ⇒  证不出来，照旧走「无 ctor」路径
```

**已知残留缺口（有意保留）**：`argc == 0` 时区分不了「本来就无 ctor」与「`C()` 在旧依赖里
不存在」。

原先记的补法是「让 `TypeDesc` 记录本类声明了哪些构造器」，**经复核几乎没有覆盖面**：
primary 构造器用**裸键**，所以「类只要声明了任何构造器，裸键就一定解析得到」——裸键解析
不到时，被加载的那份类几乎必然真的零构造器，新元数据永远判不出问题。

真正的闭合要在**调用点**给「零构造器」一个可区分的编码（如 `IrLoopAllocReuse` 早已确立的
空 ctor 名 = 裸分配），于是「非空却解析不到」⇒ 无论 argc 都是缺失。卡点是**判据算不准**：
z42c 为「有字段初始化器、无显式 ctor」的类**合成**的隐式构造器（`IrGenTypeEmitter._emitSynthCtor`）
既不是 `MethodSymbol`、也不在 `_bindNew` 那一刻存在（`_synthCtors` 跑在所有绑定之后），
而准确的 oracle（本包全部已发射函数 ∪ `DependencyIndex.Statics`）要等整包装配后才齐。
判错的代价是**静默跳过真构造器**——比它要修的 bug 更坏。故另立 change 处理。

### 构造器（续）—— `wrong_ctor_arity_exception`，解析**成功**也要查

上面那条判据只管「解析不到」。但同一个裸键在版本 skew 下还会**命中错的构造器**：

| | 编译时依赖（v2） | 运行时加载到（v1） |
|---|---|---|
| 声明 | `class Widget { Widget() {...} }` | `class Widget { Widget(int v) {...} }` |
| 裸键 | `Demo.T.Widget.Widget`（primary） | `Demo.T.Widget.Widget`（primary） |

`new Widget()` 发裸键、`argc == 0` ⇒ 运行期**解析成功**，命中 `Widget(int)`。而
`exec_function` 用 `Frame::new(args, max_reg)` 建帧、**不做任何 arity 校验**，形参 `v` 的
寄存器就停在默认值上继续跑（实测字段被静默写成 `0`）。这不是「缺符号」，是**静默调错
构造器**——而且比 `argc == 0` 那条缝常见得多：**任何**「构造器签名变了」的 skew 都落在这里。

判据全部来自**已有**元数据（`Function::param_count` / `min_arg` / `params_from`），无格式改动：

```
phys = argc + 1                       // 调用方实际传入的值个数（含 this）
min  = min(min_arg + 1, param_count)  // ⚠️ 见下
max  = params_from != 0xFF ? ∞ : param_count
phys ∉ [min, max]  ⇒  抛
```

> ⚠️ **`min_arg` 两种口径并存，必须夹住。** 文档口径是**逻辑**必填数（不含 `this`），
> `IrGenFacts._fillParamMeta` 写的也是逻辑值；但 `IrFunction` 构造器的**默认值**是
> `MinArg = paramCount`，那是**物理**总数（含 `this`），`IrGenMemberEmitter` 的注释明说了
> 这点并为 getter/setter 手工覆盖。没被 `_fillParamMeta` 覆盖过的合成函数因此会多算 1，
> 不夹住就会把合法构造判成 skew。夹到 `param_count` 后默认情形退化成「全必填」——正是
> 那个默认值本来的语义。回归钉在 `vm_context/symres_tests.rs`。

复用 `MissingSymbolException` 而不新增异常类：新类要先进 stdlib，而冷启动种子的 stdlib
里没有它 ⇒ 得走两-nightly。语义上也说得通：调用点指名的那个重载**确实不在**，撞上的是
同键下的另一个。

**两个后端都查，且 JIT 的 native 分支不能漏**——跨包构造器正是惰性加载、最容易 tier 到
native 的那批。区间在 `FnEntry` 里随编译一次算好（`jit/lazy.rs`），于是两条分支都不必为
每次构造再查一遍函数元数据。

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

用编译期宏 [`available!(X)`](../language/available-macro.md) 把那条分支保护起来：被保护的
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
