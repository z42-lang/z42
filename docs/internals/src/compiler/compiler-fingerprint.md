# 编译器语义指纹（`CompilerFingerprint`）

> 页型：机制页 ｜ 状态：✅ 已实现 ｜ 代码：`src/compiler/z42c.pipeline/src/CompilerFingerprint.z42`
> 规则见 [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)「编译器语义指纹」。

## 它是什么

增量缓存的失效判据里，「源内容哈希 + zbc/zpkg 格式 Minor」测不出**「编译器语义变了但格式没变」**。
指纹补的就是这个次元：它进 `.meta` 的 `z42c-fp` 行与 `package.meta` 头，不符即令条目作废。

## 它怎么算（2026-09-27 起：内容派生）

**指纹 = `CompilerFingerprint.Entries` 这张列表的内容哈希**（`ZpkgBuilder.SourceHashHex`）。
每条语义变更**追加一行自己的 slug**，不再手工 +1。

```z42
public static string[] Entries = new string[] {
    "baseline-41",
    "fingerprint-content-derived",
    // 新变更追加在末尾，一行一条
};
```

### 为什么换掉那个计数器

计数器有两个实测损害，**都在 2026-09-27 当天发生**：

| 损害 | 实录 |
|---|---|
| **撞号 / 让号** | 同一天两次：#883 原取 36 被 #885 抢走让到 37；#892 原取 38 被 #891 抢走让到 39。号是「先合先得」，后到的必须改自己的 PR |
| 🔴 **正文被整行覆盖而丢失** | #897（指纹 40，E0403）的**整条理由**被 #898 的合并吃掉 —— 两个 PR 都基于 39、都改**同一行**，后合的整行胜出，**git 没报冲突**。main 上一度是 41 → 39，40 那一档凭空消失（`NeverCompletes` 在链里零出现） |

列表方案把两者结构性消掉：**没有号可抢**；两个 PR 各追加一行，合并只会**两行都留下**（要么自动合并，要么冲突时正解是「都留」而不是「谁让」）。

⚠️ **顺带说明为什么不去哈希编译器全部源码**：实测那会让**每次注释编辑**都全量失效 ——
空跑 12.0s / 改一行注释 18.0s / 指纹变一次 **26.8s**（`cached: 0/123`），即内循环 +50%，
还连带 stdlib 全量。列表方案只在**人记录了一条语义变更**时才变，零额外失效。
（代价：仍需人判断「这次要不要记一条」—— 与计数器时代相同，由 CI 守门兜底。）

## 什么时候要追加一条

判据不变，见 [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)。一句话：
**同一份源码的编译结果（含诊断集）是否可能变**。⚠️ 注意「诊断变、发码不变」也算 ——
那一档 **CI 的 fingerprint 守门是瞎的**（它比的是产物字节），只能靠人记。

## 历史（计数器时代，1–41）

下表是换方案前的全部条目，**按号降序**。保留它们是因为每条都记着「为什么这次必须失效缓存」，
那些理由本身就是判据的语料。


### 41　enforce-catch-type

enforce-catch-type：**`catch (T e)` 的 `T` 必须是 `Exception` 或其子类（E0420）**。此前完全不校验：`catch (NotAnException e)` **编译零诊断**，而运行期那个 catch **静默永不匹配** ⇒ 异常穿出去变成 `uncaught exception`（实测：抛 `Std.Exception`、catch 写 `NotAnException` ⇒ 程序打完 `start` 就死在 `uncaught exception: Std.Exception: boom`）。`error-codes.md` 早就如实标着「⚠️ 零发射点 —— catch 类型当前不校验」。🔴 **宽松闸门**：本次编译找不到名为 `Exception` 的类时一概不报 —— 不链 stdlib 的路径真实存在且合法（`SemanticDump.FirstErrorCode` 的独立 Infer 路径），那里 `Exception` 根本不存在，没这条闸门所有 catch 类型都会被误报。⚠️ **这条的保守方向与 E0403（add-missing-return-check）正好相反** —— 同一个「保守」对不同检查指向不同方向：那边「拿不准就报」是误报源，这边「拿不准就报」也是。⚠️ **bump 的理由是诊断变**：写错 catch 类型的源文件此前**编得过、零诊断**，现在报 E0420，哈希一字未变 ⇒ 不 bump 会命中旧条目、把新诊断吞掉。发码零变化 ⇒ CI 的 fingerprint 守门对这一档是瞎的。⚠️ **让号**：main 上是 39，在飞的 #897（E0403）取 40 ⇒ 本刀 41。全仓零 E0420（编译器 / stdlib 25 包 / xtask / workload+toolchain / 382 golden / 347 stdlib 测试）。

### 40　add-missing-return-check

add-missing-return-check：**非 void 函数所有路径必须 return（E0403）终于发射**。漏 return 的函数此前编得过、零诊断，运行期返回 `Null`，调用方在毫不相干的位置崩成 `VCall: expected object, got Null`。新写反向保守的 `NeverCompletes`（16 种 `BoundStmt` 逐个分类、无「拿不准」档），不复用 `AlwaysReturns`（它的保守方向对本检查是反的）。bump 理由是诊断变。⚠️ 本条正文于 2026-09-27 被 #898 的同行合并覆盖而丢失，在 fingerprint-content-derived 里补回 —— 这正是计数器方案被替换的直接原因。

### 39　fix-devirt-bare-name-alias

fix-devirt-bare-name-alias：**去虚化不再把「裸名别名」当直呼目标名**。`DependencyIndex.AddModule` 给 mangle 名（`Name$N$T`）**额外注册裸名别名**（`Cls.Name` 与 `ns.Cls.Name`），供调用点按裸名消歧；而 `_depHasFunction` 只查**键是否存在**，它的两个调用方（`ResolveSealedTarget` / `NarrowPrimTarget`）却把 `fq` 直接当直呼目标名 ⇒ 命中一个别名就发出一条指向**不存在的函数名**的直接 Call。实测 4 行可复现：`typeof(int).GetType().FullName` 在 `--opt-all` 下报 `undefined function \`Std.Type.GetType\``（真身 `Std.Type.GetType$1$string`：`Std.Type` 声明 `static extern Type GetType(string)`，而裸名 `GetType` 又被 TSIG 从 `Std.Object` 的实例方法展平进 `Type.Methods`，两者在裸名上撞车）；`--release --no-opt devirt` 通过 ⇒ 单变量锁定 Devirt。判据收紧为「entry 的**真名**与 fq 逐字相同」：非重载方法照旧命中（零回归），只有「别名命中而真名不同」这一格被拒 —— 那一格本来就必错。⚠️ **bump 的理由是发码变**：受影响的调用点从「直接 Call 到一个不存在的名字」改为回落 VCall ⇒ 同一份源码哈希不变而发码变，不 bump 就会命中旧条目、修复永不生效。⚠️ **文档此前的「永不 miscall」是假声明**（`optimization-pipeline.md` 与 `sealed.md` 各一处），已一并改正。⚠️ **让号实录**：main 上是 37，在飞的 #891 取 38 ⇒ 本刀让到 39（parallel-development.md §4.1）。

### 38　fix-inline-breaks-ref-params

fix-inline-breaks-ref-params：**内联不再破坏 `ref` 形参协议**。`ref` 在运行期靠「callee **入口** copy-in / **出口** copy-out」实现（`exec_function_body` 把持 `Value::Ref` 的形参槽解引用，`run_ref_writebacks` 在每条退出路径写回 caller 的 lvalue），而**内联把 callee 帧整个去掉了** ⇒ 没有入口解引用、没有出口写回 ⇒ 裸 `Value::Ref` 直接流进 body 的算术。实测仓库自己的 `src/tests/refs/ref_local`（10 行）：`z42c build --release` 后运行期 `type mismatch in arithmetic: Ref { idx: 0, frame_id: 1 } vs I64(1)`，而 debug 通过；`--release --no-opt inline` 通过、`-O0 --opt inline` 复现 ⇒ **单变量锁定 Inline**。修法取**调用点侧**判据：实参由**任一取址指令**产生（`LoadLocalAddr`/`LoadElemAddr`/`LoadFieldAddr`）即拒绝内联该调用点 —— 因为 **callee 的 IR 里根本看不出哪个形参是 `ref`**（escape-analysis.md 记过同一条信息缺口），这样不必给 IR 加 per-param 标志、**不动 zbc 格式**。⚠️ **既有的「被写形参材料化」救不了它**：那条 emit 的 `copy (p+offset), arg[p]` 拷的是**地址的副本**，不是被指向的值；它解决的是「别把写踩到调用方实参寄存器」，与解引用无关。⚠️ **三种取址指令必须全覆盖**：只判 `LoadLocalAddrInstr` 时实测 4/7 个 refs fixture 转绿，`ref` 指向**数组元素 / 对象字段**的三个照旧崩（本仓反复出现的「只做一格漏掉常见形态」）。⚠️ **bump 的理由是同一份源码的发码变**：带 `ref` 实参调用的源文件此前**编得过、只是运行期崩**（或在非算术路径上静默错），现在那些调用点不再被内联 ⇒ 哈希一字未变而发码变，不 bump 就会命中旧条目、修复永不生效。z42c 自己有 40 处 `ref` 调用 ⇒ **这一档 CI 的 fingerprint 守门看得见**（自举产物会变）。

### 37　converge-prim-wrapper

converge-prim-wrapper：「prim 名 → 包装类短名」此前有**三份并行实现**且已不一致 —— `PrimModel.Wrapper`（权威，自称已收敛七张映射）、`TypeFactsTc._primWrapper`（独立 13 条 if 链 + `_capFirst(n)` 回退）、`EmitContext._primWrapper`（13 条 if 链 + 原样回退，注释声称「镜像 `TypeChecker._primWrapper`」而**那个函数早已不存在**）。后两份改为转发到 `PrimModel.Wrapper`，`_capFirst` 随之成死码删除。⚠️ **bump 的理由是映射答案对三类输入变了**（非内建小写名不再被猜成 PascalCase；`object` 在 EmitContext 侧终于映射到 `Object`；`int?` / `Std.Int32` 经 `Canon` 归一）⇒ 旧缓存条目理论上可与新编译结果不同，不 bump 有误命中风险。⚠️ **实测这三处分歧都不是可观察的活 bug**：自举不动点 3/3 **逐字节一致**，且用 `class Point` + `class point` 并存的可执行探针在**新旧 driver 上输出相同**（`2/1`）⇒ 本条是「去掉两份并行真相 + 一份有损猜测」，**不是** bug 修复，别在别处引用成后者。 ⚠️ **让号实录**：本 PR 原取 36，而 **#885（qualified-base-name）先合入 main 取走 36** ⇒ 按 parallel-development.md §4.1「按合并顺序让号」坐实 **37**（指纹是单调的缓存失效计数器，跳号无害）。

### 36　qualified-base-name

qualified-base-name：**基类型写限定名不再静默丢掉继承关系**。`StubCollector` 的基表分流直接拿**源码原样串**查 `Interfaces`（键是裸短名）⇒ 限定名一律查不中。一个根四个症状（现建编译器实测）：① `class A : Std.IDisposable` 接口关系丢失报 E0402；② `class C : Demo.ILocal` **同包**限定名一样中招（是拼写问题、不是跨包问题）；③ 🔴 `class E : Std.IDisposable, Base` 接口先占了 `hasBase` ⇒ **真基类被静默吞掉**（`e.Tag()` 报 E0401）；④ 🔴 `class G : Demo.Base` 整条基类关系丢失。⚠️ **判定此前有两份且同错**（符号层 `_passClassStubs` + 发射层 `ClassDescBuilder._classDesc`）——**只修一份反而升级成运行期崩**：实测只修符号层后 ③ 编译通过而运行期 `VCall: Demo.E.Tag not found`。现收敛到 `SymbolTable.InterfaceKeyOf` 一个出口。⚠️ **bump 的理由是「修前也编得过」那两档**：③④ 修前**零诊断**、照常发码，只是发出错的类描述符（基类指向不存在的类型）⇒ 那类源文件**哈希一字未变而发码变**，不 bump 就命中旧条目、修复永不生效；①② 不构成理由（编不过的源文件没有缓存条目）。全用现有机制 ⇒ **无格式 bump**。⚠️ 取号：main 上是 35，无在飞 PR 取号 ⇒ 本 PR 取 36；合并前按当时的 main 现查复核。

### 35　（无 slug）

：fix-flow-accessor-bypass：属性 getter/setter 与索引器 getter/setter 四处此前**直接调 `_bindStmt`**、绕过 `_bindMethodBody` ⇒ `FlowAnalyzer.Check` 不跑 ⇒ **E0407 与整套 `?` 空检查在属性/索引器体里全是哑的**。现在四处都过一遍流分析（合成 `MethodDecl` 壳供 `_seed` 取种子）。⚠️ **bump 的理由是诊断变**：属性体里「局部未赋值就读」此前**编得过、零诊断**，现在报 E0407 —— 那类源文件**哈希一字未变** ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉（与 #791 / #806 / #850 / enforce-null-at-cast 一字不差的理由）。⚠️ **stdlib 25 包 + z42c 自建零命中** ⇒ 产物一字不变 ⇒ **CI 的 fingerprint 守门对这一档是瞎的**，按 version-bumping 规则表第 1 行手动 bump。实测阴阳对照：同一份 `class C { int P { get { int x; return x; } } }`，撤回修复 ⇒ 编译通过零诊断；带修复 ⇒ `E0407: use of unassigned local variable \`x\``。

### 34　interface-assignability

interface-assignability：**接口 → 祖先接口的赋值关系**终于成立，外加**实例化接口不再按裸名混同**。前者：`Conversion.Classify` 备齐了 class→iface（F）、inst→iface（G）、class→class（E），独缺 iface→iface ⇒ `IBase b = derivedIface` 报 E0402（实测七处全中：var-decl / 实参 / 返回 / 隔一层 / 跨包 / 泛型接口 / 接口实现的返回协变 E0412）。后者：`Z42InterfaceType.IsAssignableTo` 比的是 `Name()`，而 `Z42InstantiatedInterfaceType.Name()` **刻意返回裸名** ⇒ `IBox<int>` 与 `IBox<string>` 判成同一个。判定收敛到 `InterfaceClosure.IsInterfaceSubtype`（走 `BaseAt`，它本就解析声明形态并逐层代换实参 ⇒ 限定拼写与链上型参两件事一起对），六个消费点全改走它。⚠️ **bump 的真正理由不是「此前编不过」**（那类源文件没有缓存条目）——是**重载择优**：`OverloadResolver._refUpcast` 拿 `Classify` 打分，实测 `F(object)` + `F(IBase)` 传 `IDerived` 今天绑 `F$1$object`、修后 `F(IBase)` 更具体改绑 `F$1$IBase` ⇒ 那类源文件**哈希一字未变而发码变**，不 bump 就会命中旧条目。理由二：实例化接口那条是**静默错值**（实测 `IBox<int> i = iboxOfString; int bad = i.Get();` 编译零诊断，跑出 `bad = hello`、`bad + 1 = hello1`，exit 0）⇒ 修前编得过、修后报 E0402，哈希不变而结果变。全用现有机制 ⇒ **无格式 bump**。⚠️ **让号实录**：本 PR 原取 33，与在飞的 #866（carry-iface-null-marks）撞号先让到 34；随后 **main 也落到 33**（#874 fix-nested-generic-field-layout）⇒ 34 正好坐实（parallel-development.md §4.1「按合并顺序让号」；指纹是单调的缓存失效计数器，跳号无害）。

### 33　（无 slug）

：fix-nested-generic-field-layout：布局层的字段类型代换从**只整名匹配**改为**递归进实例化实参**（`Loc<T,long>` + `{T→P2}` → `Loc<P2,long>`），重组走 `StructLayout.InstName`。⚠️ **bump 的理由是同一份源码的编译结果会变**：`InstDiffersFromDef` 对「字段类型是实例化」的泛型 struct 答案翻转 ⇒ 这类实例化从「不特化」变「特化」，布局从 16B 变 24B。修前实测崩在 `struct field write out of blob bounds (off=16, w=8, len=16)`（分配端按擦除布局给 16 字节、访问端按实例化布局写 off=16）—— 不 bump 就会命中旧条目、把坏 zbc 留着。⚠️ **stdlib / z42c 里没有这种形状**（`ValueTupleN` 的字段是裸型参、不是实例化）⇒ 产物一字不变 ⇒ **CI 的 fingerprint 守门对这一档是瞎的**，按 version-bumping 规则表第 1 行手动 bump。

### 32　enforce-null-at-cast

enforce-null-at-cast：**转换到不可空值类型**现在也算解引用点 —— 标了 `?` 的值不检查就 `(int)x` / `(long)x`，此前**编得过**、只在运行期抛 `NullReferenceException`，现在报 E0478 / E0484。界线与运行期一致（#746）：转**值类型**该报、转**引用类型**放行（null 合法得 null），`as` 不介入。⚠️ **bump 的理由是诊断变**：那类源文件**哈希一字未变**、此前零诊断 ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉（本线**第六次**栽在缓存上，理由与 #791 / #806 / #850 一字不差）。⚠️ 发码本身不变（诊断不改 IR）⇒ CI 的 fingerprint 守门测不到这条（只会漏判），按 version-bumping.md 手动 bump。

### 31　（无 slug）

：make-ref-hard-cast-checked：**引用类型的硬转换终于受检**。`(B)o`（o 其实是 A）此前**不抛、返回非 null** —— 发射层对引用目标是 `toIr == IrType.Ref → return src`，**一条指令都不发**，错类型的对象原样流下去、到很远的地方才以别的面目崩。值类型那半 #746 已修，引用这半一直欠着。现在合成 `is_instance` + 分支 + `ObjNew` + `Throw`（null 按 C# 放行）⇒ 类型不符抛 `Std.InvalidCastException`，可 catch。🔴 判据有两格：主判据 `ConvKind.ExplicitRef`（基→派生），外加**从 `object`/接口 强转到引用类型**—— 后者 `Conversion.Classify` 给的是 `None`（`IsSubclassOf` 不认 `Std.Object` 这条隐式根基），只做第一格会恰好漏掉最常见的 `object o; (B)o`（实测，我第一版就是这样）。⚠️ **bump 的真正理由**：含引用下转的源文件**哈希不变而发码变**（从零指令变成一段受检分支），不 bump 就会命中旧条目、这条修复对它们永不生效。全用现有指令 ⇒ **不新增 opcode、不动 wire 格式、VM 一行不改**。 ⚠️ 让号实录：本刀原取 29，连续两轮 rebase 中 main 先后被 #851/#853/#856、#854 取走 ⇒ 最终让到 31（parallel-development.md §4.1「按合并顺序让号」）。

### 30　（无 slug）

：single-field-struct-value-semantics（坑点 ⑤）：**单字段 struct 从引用语义改为值语义** —— `StructLayout.IsBlobStruct` 的闸门 `FieldCount >= 2` 翻成 `>= 1`，单字段 struct 与多字段走同一个值模型（arena 字节 + 逐叶子复制 + sret + `__box_struct`）；配套 VM 侧那份**逐字镜像**的闸门（`interp/exec_array.rs::try_struct_backed`）同步翻，以及 `extern` 桩新增 blob 返回的 sret 通道（`_emitNativeStubSret`，全仓唯一这种形状是 `Std.GCHandle.Alloc`）。⚠️ **bump 的真正理由**：`S b = a; b.X = 50;` 这类源文件**修前编得过、跑得过**（只是静默改到了 `a`）⇒ 哈希一字未变而发码全变（现在发 `struct_alloc`/`struct_copy`/`struct_fget_prim` 而非对象字段读写），不 bump 就会复用旧编译器的缓存条目、值语义对它们永不生效 —— 这是**静默错值**类，比崩溃更需要 bump。⚠️ 本档 stdlib 产物**确实会变**（`Std.GCHandle` / `Std.Guid` 是单字段 struct）⇒ CI 的 fingerprint 守门这次应当看得见。⚠️ **让号实录**：main 上是 29（本人 #851 那一档让号后的值）⇒ 本 PR 取 30；合并前按当时的 main 现查复核。

### 29　（无 slug）

：fix-crosspkg-static-sret：`CallEmitter` 的**静态**捷径（DepIndex static 调用 + 静态属性访问器的依赖分支）此前直接发裸 `CallInstr`、**不处理 sret** ⇒ 跨包调用返回 blob 值 struct 的静态方法/属性，生产方按 sret 编、消费方少传一槽 ⇒ 编译期零诊断、运行期 `takes N+1 physical argument(s), the call passes N`。两条**实例**捷径（devirt / DepIndex instance）一直是对的，只有静态这条漏了。⚠️ **bump 的真正理由**：这种写法**修前编得过**（照常发码），只是运行期崩 ⇒ 那类源文件**哈希不变而发码变**（现在多一个 `struct_alloc` + 末尾 sret 实参），不 bump 就会复用旧编译器的缓存条目、修复对它们永不生效。⚠️ **CI 的 fingerprint 守门对这一档也是瞎的**：实测 `z42.core` 里**一个多字段 struct 都没有**（只有 `GCHandle`/`Guid` 各 1 字段 + 12 个零字段基元 wrapper）⇒ stdlib 根本走不到这条路、产物一字不变，按 version-bumping.md 手动 bump。⚠️ **让号实录**：main 上是 25（#846），在飞的 #845 与 #847 **双双取 26** ⇒ 本 PR 先取 28，**而 main 随后也落到 28**（#845/#850 那一波）⇒ 再让到 29（parallel-development.md §4.1「按合并顺序让号」）；合并前仍按当时的 main 现查复核。

### 28　（无 slug）

：carry-field-null-marks：**字段 / 属性**的 `?` 标记现在跨包携带（骑字段自己的 attr-ref 通道发 `$Nullable` 哨兵，**无格式 bump** —— 同 #791 形参 / 返回值那两档）。⚠️ **bump 的理由是诊断变**：跨包读一个标了 `?` 的字段 / 属性并直接解引用，此前**编得过**（导入侧标记位恒 false ⇒ 漏报），现在该报 E0478 / E0484，而那类源文件的**哈希一字未变** ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉（这条线已经**第五次**栽在缓存上，理由与 #791 / #806 一字不差）。配套理由二：标了 `?` 的字段其 attr 块多了哨兵 ⇒ 编出的 zpkg 字节变。⚠️ **让号实录**：本 PR 先后取过 23 / 26，两次都被先合的 PR 抢走 ⇒ 按 parallel-development.md §4.1「按合并顺序让号」最终坐实 **28**（指纹是单调的缓存失效计数器，跳号无害）。

### 25　（无 slug）

：unify-blob-return-abi：**blob struct 返回位的 ABI 统一** —— 接口声明的返回位是**型参**（含 `Self`）时，调用点（擦除的泛型体 / 接口收者）把它当引用槽、不传 sret，而返回 blob struct 的具体实现有 sret ⇒ 运行期 `takes 3 physical argument(s), the call passes 2`（`where T : INumber` 对双字段 struct）与 `takes 2 …, passes 1`（实例 `Self Copy()` 经接口收者）。修法沿 add-iface-return-bridge 的 A′：桥接占裸名（无 sret、`__box_struct` 返回）、具体实现挪 `<m>$struct`，并把四处仍以裸 `RegKey` 作调用目标名的**静态**调用点改走 `CallKey()`。⚠️ **bump 的真正理由**：这类写法**修前编得过**（编译期零诊断、照常发码），只是运行期抛 arity 失配 ⇒ 那类源文件**哈希一字未变而发码变**（现在多一个桥接函数、具体实现换了名、具体收者调用点指向新名），不 bump 就会复用旧编译器产出的缓存条目、桥接根本不存在。全用现有指令与既有 `$struct` 通道 ⇒ **无格式 bump**。⚠️ **CI 的 fingerprint 守门对这一档是瞎的**：实测 stdlib 里 `INumber` 的实现者只有基元 wrapper（scalar，`CanBridge` 判假 ⇒ 不桥接不改名），返回 `Self` 的接口成员也只有 `INumber` 那五个 ⇒ stdlib 产物一字不变，守门只会漏判，按 version-bumping.md「这类改动仍手动 bump」。⚠️ **让号实录**：main 上是 22，而在飞的 #844 与 #845 **双双取了 23**（其中一个必须让号）⇒ 本 PR 让到 25（parallel-development.md §4.1「按合并顺序让号」）；合并前需按当时的 main 现查复核。

### 23　（无 slug）

：fix-inherited-typeparam-field-type：从**闭合泛型基类**继承来的字段，其类型现在按基类实参代换（`class DInt : GBox<int>` 的 `d.V` 是 `int`，不再是裸 `T`）。⚠️ **bump 的真正理由是「修前也编得过」的那一档**：报错档（`d.V + 1` / `if (d.V)` 报 E0402）本来编不过、不存在缓存条目；但**赋给具体类型的局部**（`int y = d.V;`）修前**静默通过** ⇒ 那类源文件**哈希一字未变而发码变**。实测 A/B（两个二进制、同一份源码）：zbc 942 字节里差 **2 个字节** —— offset 713 的 zbc 类型 tag `0x00`→`0x04`(I32)、offset 816 的 REGT `0`→`3`(`IrType.I32`)，即「类型未知」变成「具体类型」；IR 文本一字不差（差异全在元数据）。不 bump 就会复用旧条目，那里的 REGT 是 Unknown，优化/JIT 只能走保守路径。⚠️ stdlib 与编译器自身没有「派生于闭合泛型基类」这种形状 ⇒ CI 的 fingerprint 守门测不到这条路径（只会漏判），按 version-bumping.md 手动 bump。⚠️ **让号实录**：本 PR 原取 22，与在飞的 #843 撞号；#843 先合入 main 取走 22（`8d02c5076`）⇒ 按 parallel-development.md §4.1「按合并顺序让号」坐实 23（指纹是单调的缓存失效计数器，跳号无害）。

### 22　（无 slug）

：complete-generic-class-identity 收尾：**模式匹配的类型测试也携类型实参**。#831 让 `is`/`as`/cast 认了实参，模式那条路还在用擦除名 ⇒ 同一个问题两个答案：`o is Box<string>` 说 false，而 `switch (o) { case Box<string> b: … }` **匹配上**，`b` 拿到一个装着 int 的 `Box<string>` —— **静默**，还把错类型漏给下游（实测三种模式形态全中）。根因是两把尺子：`TypeOpEmitter` 与 `PatternEmitter` 各写了一份 `QualifyTypeName` + Array/Object 归一，#831 只改了前者；现收敛到 `ExprEmitter._typeTestName` 这**一个出口**。⚠️ **bump 的真正理由**：这次改的是**发码** —— `is_instance` / `as_cast` 在模式路径上的目标名由 `Demo.Box` 变成 `Demo.Box<int>`，含泛型模式的源文件**哈希不变而发码变** ⇒ 不 bump 就会命中上一版编译器的缓存条目、误匹配对它们永不修复。无格式 bump（全用现有指令）。⚠️ stdlib / 编译器里没有「对着错实例化做模式匹配」的形状 ⇒ CI 的 fingerprint 守门只会漏判，手动 bump。

### 21　（无 slug）

：fix-ctor-param-resolved-in-caller-scope：构造器形参的声明类型含**型参**时，实参检查此前在**调用点**的环境里重新解析那个类型（`OverloadBinder._adaptParamType` 的 `md != null` 分支走`env.ResolveType(md.Params[i].Type)`），而型参在调用点根本不存在 ⇒ 解析成 `Unknown`。同一根因两副面孔：嵌套 `G<U>` 变 `G<<unknown>>` 与实参结构比 ⇒ **误报**（`class Wrap<U> { Wrap(G<U>) }`这个类根本构造不出来）；裸 `U` / `U[]` 被 `Unknown` 吸收 ⇒ **漏报**（错类型静默放行）。修法：含型参的形参位**不走这条检查**，交给按实参代换后的补查（`ConstructTyper._chkCtorSubstArgs`）—— 与既有纪律「只补查原签名含型参的那些位」正好分区，不重不漏。⚠️ **bump 的真正理由是诊断变**：**漏报那一侧的源文件此前编得过（零诊断）、现在该报 E0402，而它们的哈希一字未变** ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉（这条路数本仓已栽过四次）。误报那一侧不构成理由（编不过的源文件根本没有缓存条目）。发码零变化（自举字节不动点 3/3 复验）。⚠️ **CI 的 fingerprint 守门看不见它**（纯诊断变化、stdlib 产物一字不变）⇒ 按 version-bumping.md 手动 bump。

### 20　（无 slug）

：complete-generic-class-identity P3 + P5-b：**静态成员按闭合类型各一份**（对齐 C#）。静态字段键由擦除名改为实例化名（`Demo.GBox<int>.Count`）、成员体按实例化各发一份（闸门 `IrGen.InstNeedsOwnBody` = 布局不同 **或** 定义有静态状态）、类型初始化器同样各一份；解析器补上 `GBox<int>.Count` 这个写法（P5-b）。⚠️ **bump 的真正理由**：这类源文件**哈希一字未变而发码变**（`static_get`/`static_set` 的键换了、多出按实例化的成员体与 `$cctor`），不 bump 就会命中上一版编译器的缓存条目、「两个实例化共用一个静态槽」这条静默错值对它们永不修复。全用现有指令 ⇒ **无格式 bump**。⚠️ **CI 的 fingerprint 守门对这一档是瞎的**：实测全仓 139 个泛型类型声明里**带静态字段的是 0 个**（stdlib / 编译器 / 工具链 / 测试 / 示例全扫过），stdlib 产物一字不变 ⇒ 守门只会漏判，按 version-bumping.md「这类改动仍手动 bump」。同一条实测也说明**不需要分阶段引入**：「生产方与消费方必须同代编译器」那个危险在自举路径上没有任何实例。

### 19　（无 slug）

：complete-generic-class-identity P1+P2：泛型 class 的**实例化成为运行期真正的类型** —— 每个具体的本包实例化都发一条**完整**类描述符（基类链 / 接口按实参代换、字段类型名代换），闭合泛型基不再被剥成裸名，`is`/`as` 的目标名改走实例化身份名（此前取 AST 的 `NamedType.Name`、丢掉类型实参）。⚠️ **bump 的真正理由**：含泛型的源文件**哈希一字未变而发码变** —— TYPE 段多出实例化描述符、`class D : G<int>` 的 base 由 `G` 变 `G<int>`、`is_instance`/`as_cast` 的目标名由 `G` 变 `G<int>`。不 bump 就会命中上一版编译器产出的缓存条目，于是「继承的型参字段读出 null」「`o as G<string>` 在 `G<int>` 上放行」这两条静默错值对它们**永不修复**。全用现有指令与既有 TYPE 段结构（reader 按 count 循环）⇒ **无格式 bump**。⚠️ 与前几档不同，这一档 **CI 的 fingerprint 守门真的看见了**（z42.core 输出变，本 PR 正是被它判红后才补上这次 bump）—— 那是好事：说明 stdlib 里确实有这种形状，不是只会漏判的那类。

### 18　（无 slug）

：fix-foreach-dispose-inherited：foreach 枚举器路径的 `Dispose` 条件判据改问**继承面** （类沿 `BaseName` 链、接口交 `InterfaceClosure.FindMethod`），不再只查 `MethodsOf` 给的那张**直接**成员表。⚠️ **bump 的真正理由**：#823 引入条件化后，`Dispose` 从父接口或基类继承来的源文件**编得过**、但发码里**少了那次 `Dispose` 调用**（实测 `Std.IEnumerable<T>` 那条路 `disposed=0`，具体类型是 1）⇒ 这类源文件**哈希不变而发码变**，不 bump 就会命中 #823 那版编译器产出的缓存条目、修复对它们永不生效。⚠️ 让号实录：#820 在飞时已取 17 ⇒ 本 PR 让到 18（parallel-development.md §4.1「按合并顺序让号」）。

### 17　（无 slug）

：complete-generic-instantiation S1：**泛型体的单调化** —— 泛型自由函数 / 静态·实例泛型方法（含虚与 override）按**具体类型实参**各特化一份（体内布局查询按代换后的类型算，偏移随实例化烘焙），与实例化类型的特化共用同一个不动点闭包。修的是 #774 留下的**静默错值**：泛型体只编一份、按擦除布局烘焙偏移，而调用方按实例化布局造值（实测 `ReadSecond<P2>(Loc<P2,int>)` 读出 2 而非 7；虚派发读出 102 而非 107）。全用现有指令、**无格式 bump**。含泛型实例化的源文件**哈希不变而发码变**（新增特化体、调用点改派特化名）⇒ 不 bump 就会复用旧编译器产出的缓存条目（那里调用点仍指向擦除体）。

### 16　（无 slug）

：add-iface-return-bridge：接口返回位的 struct 协变现在**真能跑** —— 实现返回具体 struct、接口声明返回引用型时，具体实现挪到 `<m>$struct`、合成桥接占裸名 vtable 槽（`struct_alloc` + `call $struct` + `__box_struct` + `ret`），类收者调用点经 `MethodSymbol.CallKey()` 指向 `$struct`、接口收者仍走裸名。⚠️ **bump 的真正理由**：这种写法**修前编得过**（编译期零诊断、照常发码），只是运行期抛 `takes 2 physical argument(s), the call passes 1` ⇒ 那类源文件**哈希不变而发码变**（现在多一个桥接函数、具体实现换了名），不 bump 就会复用旧编译器产出的缓存条目、桥接根本不存在。⚠️ 这一档 stdlib 现在**真的用上了**（`List<T> : IEnumerable<T>` + `ListEnumerator<T> : IEnumerator<T>`）⇒ 与 #795/#796 那种「stdlib 无此形状、CI 守门只会漏判」不同，本次 fingerprint 守门应当能看见输出变化。 ⚠️ 让号实录：main 上 15 已被 carry-null-marks-through-native-stubs 取走，本 PR 让到 16（parallel-development.md §4.1「按合并顺序让号」）。

### 15　（无 slug）

：carry-null-marks-through-native-stubs：`extern` 桩现在也写 attr-ref 块（`Attrs` / `ParamAttrs`），与 **abstract 桩**那一支对齐。此前 `IrGenMemberEmitter` 的 extern 分支压根不填这两个字段 ⇒ 源码里**已经标了** `?` 的 9 处 extern（`Environment.GetEnvironmentVariable` / `RuntimeConfig.Get`·`Describe` / `AppProperties.Get`·`Raw` / `ProcessNative.Which` 等）过了包边界哨兵就没了，导入侧标记位恒落 false。⚠️ **bump 的真正理由是诊断变**：跨包调用这些 extern 并直接解引用的源文件**此前编得过（零诊断）**、现在该报 E0478/E0479，而它们的**哈希一字未变** ⇒ 不 bump 就会命中旧条目、把新诊断整个吞掉（本线第四次栽在缓存上）；配套理由二：标了 `?` 的 extern 桩 attr 块多了哨兵 ⇒ zpkg 字节变。⚠️ 全用现有通道（骑 `$ByRef`/`$Deprecated` 那条 attr-ref 路）⇒ **无格式 bump**。实测：extern 声明里 `ref` 形参 0 处、默认值 0 处 ⇒ 顺带的 `$RefSig`/`$Default` 哨兵均为空转。

### 14　（无 slug）

：fix-tuple-name-substring-judgment：元组模式的判据从**名字含子串**（`IndexOf("ValueTuple") >= 0`）改为**整名精确匹配** `ValueTuple2..8` ⇒ 名字里含 ValueTuple 的**用户类型**不再被当成元组。此前实测：`struct MyValueTupleBox` 与逐字段同形的 `struct PlainBox` 只差一个名字，前者 `case (a, b)` **编译零诊断**并走 blob 偏移解构、后者报 E0402 —— 能力按名字子串分叉（漏报）。⚠️ **bump 的真正理由**：那类源文件**此前编得过**（零诊断、照常发 `StructFieldGetPrim`）⇒ 哈希不变而结果变（现在报 E0402），不 bump 会复用旧条目、新诊断永不发射。⚠️ stdlib / 语料里没有这种 lookalike 命名 ⇒ CI 的 fingerprint 守门测不到（只会漏判），按 version-bumping.md 手动 bump。　⚠️ 让号实录：#791 先合占 12（`3d525743c`）、#795 让到 13 并已合入（`dc3ec1afd`）⇒ 本 PR 让到 14（parallel-development.md §4.1「按合并顺序让号」）。

### 13　（无 slug）

：fix-unary-operator-overload：`operator X` 重载的两种不成立写法从**静默接受**改为报错（E0495）——表外符号（`!` / `~` / `&&` …）此前回落 `"op_" + op` 造出**非法名**，`fn @Vec.op_!$1$Vec` 真的被发进 zbc；一元 `-` 撞进表里的 `op_Subtract`、产出 `op_Subtract$1$T`。两者派发侧都查不到 ⇒ 方法**永远不可达**。⚠️ **bump 的真正理由**：这两种写法**此前编得过**（声明处零诊断、照常发码，只在使用处报一句无关的 `E0402`）⇒ 那类源文件**哈希不变而结果变**（现在编不过），不 bump 就会复用旧编译器产出的缓存条目、新诊断永不发射。⚠️ stdlib / 编译器自身一处这种声明都没有（全仓 20 处 `operator` 声明全是二元 + 表内）⇒ CI 的 fingerprint 守门测不到这条路径（只会漏判），按 version-bumping.md「这类改动仍手动 bump」。⚠️ 让号实录：#791 与本 PR 曾同取 12，#791 先合入 main（`3d525743c`）⇒ 本 PR 按 parallel-development.md §4.1「按合并顺序让号」rebase 时让到 13。

### 12　（无 slug）

：define-null-check-marks 3.3：`?` 标记跨包携带 —— 形参挂 `$Nullable`、返回值挂**方法级** `$RetNullable`，骑 `$ByRef`/`$Deprecated` 那条既有 attr-ref 通道 ⇒ **无格式 bump**。⚠️ **不能把 `?` 拼进 SIGS 类型名**：那串同时是**派发键**（`Find$1$string`），改它就打烂派发与全部 golden —— `FunctionEmitter._sigTypeName` 历来剥 `?` 正是这个原因（规范里「TsigTypeName 已双向拼写」是**假前提**，实测 zpkg 里一个 `?` 都没有，只有 extern 声明那条路例外）。bump 有**两条**理由，都成立：① 标了 `?` 的方法其 attr 块多了哨兵 ⇒ zpkg 字节变；② 更要紧的是**诊断变**——跨包调用点的源文件**哈希不变**，此前编得过、现在该报 E0478/E0479/E0489，不 bump 就会命中旧条目、把新诊断整个吞掉（本线第三次栽在缓存上）。

### 11　（无 slug）

：define-null-check-marks D5：`Expect("理由")` intrinsic（空检查义务的唯一显式逃生口）—— `x.Expect("r")` 现在合成 `ConstNull + Eq + BrCond + ConstStr + ObjNew + Throw`（全用现有指令 ⇒ **无格式 bump**，同

### 10　（无 slug）

：fix-foreach-protocol-detection：`foreach` 的可迭代协议判定改看**成员面**（`ForeachProtocol.MethodsOf`/`FieldsOf`），不再只认类那条继承线 ⇒ 目标静态类型是**接口**时，此前三条路径全落空、静默落数组臂发 `array_len`（运行期抛 `ArrayLen: expected array`），现在正常走枚举器/索引路径；计数成员两档（`Count`→`Length`）也收敛到单一判定点（此前路径选择只认 `Count`、发射参数认两档，判据分叉）。全用现有指令 ⇒ **无格式 bump**。含 foreach-over-interface 的源文件**哈希不变而发码变** ⇒ 不 bump 就会复用旧编译器产出的缓存条目（那里对着一个对象发 `array_len`）。⚠️ stdlib 里没有 foreach-over-interface ⇒ CI 的 fingerprint 守门测不到这条路径（只会漏判），按 version-bumping.md「这类改动仍手动 bump」。

### 9　（无 slug）

：throw-on-switch-expr-no-match：`switch` 表达式的落空块从 `Br(end)` 改为合成 `ConstStr + ToStr + StrConcat + ObjNew + Throw`（全用现有指令 ⇒ **无格式 bump**）。含不穷尽 switch 表达式的源文件**哈希不变而发码变** ⇒ 不 bump 就会复用旧编译器产出的缓存条目（那里 `result` 读未初始化槽位、跑出垃圾值）。

### 8　（无 slug）

：record-ref-in-signature：`ref` 形参经 param attr-ref 通道跨包传播（`$ByRef` 逐形参 + `$RefSig` 方法级完备性标记，骑 zbc 1.15 既有通道 ⇒ **无格式 bump**）。每个方法的 param attr 块都多了标记 ⇒ 编出的 zpkg 字节变 ⇒ 按「编译器语义指纹」必须 bump。

### 7　（无 slug）

：workspace-read-cache：workspace 成员编译改为封闭（看不见拓扑序在后的成员）——此前后序成员的同名方法让依赖索引出歧义键被剔除，封闭后解析为直接调用（z42.json 实测 Dictionary.Get/Set/ContainsKey 由派发变直调）→ 输出变，bump。

### 6　（无 slug）

：add-implicit-base-ctor-call（#663）实例 ctor 隐式 base() + 构造器继承 / 默认构造器进符号层（codegen 与 TSIG 输出变）。再

### 5　（无 slug）

：unify-ir-operand-access：REGT 收集改走统一操作数接口，void 型 extern 桩结果寄存器 Unknown→Void。
