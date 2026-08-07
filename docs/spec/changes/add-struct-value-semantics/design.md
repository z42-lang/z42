# Design: struct 值语义 —— 内联/栈布局全重构（选项 B）

> 状态：🟡 DRAFT — User 已裁决选项 B + 3a。本设计给出内联布局架构、子决策 α–θ、分阶段落地。
> 部分子决策（α 操作数模型 / γ 叶子存储 / 格式 bump 时机）标 ⚠️，需在 P1 阶段 6.5 gate 确认。

## 目标语义（C# value type）

- **内联存储**：struct 数据存在容器里（栈帧寄存器区 / 父对象 slots 区 / 数组扁平 backing），无独立堆
  身份、无 GC 托管对象。
- **复制语义**：赋值/传参/返回/存容器 = 字段级复制（值字段拷贝、引用字段拷贝引用、嵌套 struct 递归）。
- **原地可变（3a）**：可寻址位置（局部/字段/数组元素）上的 struct，其字段可原地写。
- **装箱**：赋给 `object`/接口 → 拷进堆对象（CLR 式 boxing）。

## 现状（为什么是引用语义）

见 proposal「Why」。核心：struct=`Value::Object(GcRef)`，`derive(Clone)` 克隆句柄；`obj_new` struct/class
同路径；`field_set` 原地改共享对象；编译器 `IsStruct` 仅驱动继承+反射 flag。

## 布局模型（编译器核心，字节精确 —— γ 裁决）

为每个 struct 类型计算**字节精确扁平布局**（byte offset / size / alignment），与
[packed-primitive-arrays] 的字节布局地基收敛：

```
struct P    { var x: int; var y: int; }   // size=8B, align=4, {x:@0(i32), y:@4(i32)}
struct Line { var a: P;   var b: P;   }    // size=16B, align=4, {a:@0(P), b:@8(P)}
                                           //   line.a.x → byte 0+0=0
                                           //   line.b.y → byte 8+4=12
```

- 叶子 = 基元字段（字节精确：`int`=4B/`long`=8B/`bool`=1B/`char`=4B/…，纯字节可 memcpy）或**引用叶子**
  （object/array/string/interface/func = **16B 托管句柄**，见子决策 #2，不可 raw memcpy）；嵌套 struct
  字段**递归展开**为其叶子字节序列，按对齐排布。
- 每 struct 类型产出 `{ size: 字节数, align, field_layout: name→(byte_offset, size, kind) }`，
  其中 `kind` ∈ {基元各类, arc-string, gcref-object}。
- **引用叶子的存储**：blob 里的引用叶子既需 GC 追踪、又需按种类正确 clone/drop（Arc 计数 / GcRef 屏障）
  → blob 需记录**带种类的引用叶子偏移表/位图**（GC 扫描 + StructCopy/析构分流据此，见 Decision ζ）。
  这是字节打包相对"全 Value 槽"新增的关键机制。
- **禁止递归无限展开**：struct 直接/间接含自身值字段 = 无限大小 → 编译期报错（C# 同款 CS0523）。

## IR / 运行时操作数模型

现状不变量：**1 寄存器槽 = 1 `Value`（24B）**。字节打包内联要求一个 struct 值占容器里**一段字节 blob**
（`size` 字节），不再是若干 `Value` 槽。

### ⚠️ Decision α：struct 值操作数表示（字节 blob 区间）

**推荐**：未装箱 struct 值 = 容器里的一段**字节 blob**，IR 指令以 `(base, byte_offset, size)` 引用
struct / struct 字段的字节区间。帧需要一个**局部字节区**（与 `regs: Vec<Value>` 并存或统一）存放 struct
局部；struct 临时/参数/返回也在字节区分配。新增 struct-aware 指令：

| 指令（示意） | 语义 |
|-------------|------|
| `StructCopy dst_off, src_off, size` | memcpy size 字节 + 引用叶子写屏障——赋值/传参/返回 |
| `StructFieldGetPrim dst_reg, base, byte_off, kind` | 从 blob 读一个基元叶子到 Value 寄存器 |
| `StructFieldSetPrim base, byte_off, kind, src_reg` | 把基元 Value 写进 blob 叶子（原地，lvalue） |
| `StructFieldGet dst_off, base, byte_off, size` | 取 struct 子字段区间（复制出 blob 片段） |
| `StructFieldSet base, byte_off, size, src_off` | 存 struct 子字段区间（复制入 / 原地） |

- **`Value` 不新增未装箱 Struct 变体**：未装箱 struct 纯由"字节区间"表示，不作为单个 `Value` 流转。
  装箱态回到堆 `Value::Object`（Decision ε）。
- 编译器跟踪每个 struct 临时的 `size`，在帧字节区按 `size`+`align` 分配。
- **字节区 vs Value 寄存器**：基元叶子读写在 blob（字节）↔ Value 寄存器（算术）之间搬运；这是字节打包的
  核心新机制（相对"全 Value 槽内联"多了字节⇄Value 编解码）。
- **代价**：新指令 + 字节布局元数据 + 帧字节区 → **格式 bump**（Decision η）+ 帧模型扩展。
- **备选**（均否决）：单 `Value::Struct(Box)`（选项 A，非内联，User 已否）；全 `Value` 槽内联
  （非字节密度，γ 已否）。

### Decision β：嵌套 + 数组布局

- 嵌套 struct 递归展平（上表）。
- `struct[]`：扁平 backing `len*width` 槽（P3）；元素 i 的字段 f 叶子 = `i*width + offset(f)`。
  接 [packed-primitive-arrays] 的 `ArrayBacking`（P3 增 struct backing；P5 再字节打包）。

### Decision γ：叶子存储 = 字节精确打包（User 裁决：v1 必做）

- **叶子字节精确**：`int`=4B/`long`=8B/`bool`=1B/`char`=4B/`double`=8B/引用=句柄宽，按对齐排布——逼近
  C# 内存密度。struct 存储 = 字节 blob（非 `Value` 槽序列）。
- **与 [packed-primitive-arrays] 收敛**：复用/共建其字节 `ArrayBacking` 与字节⇄Value 编解码机制；
  P3 的 `struct[]` 即字节扁平 backing。此裁决把 packed-array 字节地基**拉进本程序 P1**（struct 局部的
  字节存储）而非留作独立后续。
- **代价认知（已向 User 摆明）**：字节打包比"全 Value 槽内联"多一层字节⇄Value 编解码 + GC 需引用位图
  定位 blob 内引用叶子 + 帧字节区模型——工作量更大、风险更高，但兑现真·C# 密度。
- **裁决影响**：P1 = 字节布局地基 + 局部 struct 字节存储；不再有"P5 可选字节打包"，字节打包贯穿各存储
  阶段。

### Decision δ：对象内联（P3）

- class 的 struct 字段内联进父 `ScriptObject.slots`：预留 width 连续槽；`field_index` name→offset。
- `ScriptObject.slots` 总长 = Σ 各字段 width。FieldGet/Set 的 struct 字段走区间 get/set。
- IC（field IC）从"name→slot" 泛化为"name→offset"，width 由静态类型已知。

### Decision ε：跨调用传参/返回 + 装箱

- **传参**：struct 实参 = 把其 width 槽**复制**进调用 arg 区间（callee 收到独立副本）。
- **返回**：callee 把返回 struct 的 width 槽写进 caller 预留的 dst 区间。
- **装箱**：`struct → object/接口` = 分配堆 `ScriptObject`，把 struct 槽区拷入 → `Value::Object`
  （CLR boxing）。拆箱 = 拷回寄存器区间。`is`/`as`/`GetType` 对装箱值走其 type_desc。
  → struct 有"未装箱内联"+"装箱堆对象"两态，与 CLR 一致。

### Decision ζ：GC 根扫描 + 读写屏障（字节 blob 含引用叶子 —— User 强调，硬约束）

字节打包后 struct blob 里的引用叶子**不再是可直接扫描的 `Value` 槽**，GC 的三件事都要显式处理：

1. **引用位图定位（根扫描）**：GC 借 StructLayout 产出的**引用位图/偏移表**（随类型元数据携带）定位
   blob 内每个引用叶子的 byte offset，把它当根扫描/更新（moving GC 下需能改写 blob 内的引用）。
   帧字节区、对象 blob 区、数组字节 backing 三处都按各自 struct 类型的引用位图扫描。
2. **写屏障（write barrier）**：任何把引用写进 blob 引用叶子的路径都要触发写屏障——
   - `StructFieldSetPrim`（对引用 kind 叶子写引用）；
   - `StructCopy`（复制含引用叶子的 blob = 对目标每个引用叶子做一次引用写 → 逐引用叶子发屏障，
     或整 blob 一次批量屏障）；
   - struct 存入对象字段/数组元素（P3）时，父容器的引用叶子写。
   对齐现有 `write_barrier_field`（add-write-barriers 2026-05-21）的分代/并发假设：**每个引用叶子写都
   必须发屏障**，热路径不能漏（否则并发/分代后端漏写）。
3. **读屏障（read barrier）**：仅当 GC 后端需要（并发标记/搬移的 SATB/Brooks-style）——
   - 从 blob 引用叶子**读出**引用（`StructFieldGetPrim` 引用 kind）；
   - `StructCopy` 读源 blob 的引用叶子。
   当前 GC 若无读屏障需求则 v1 不加，但**接口预留**：blob 引用叶子的读走统一访问点（不散落原始
   指针读），使未来接读屏障只改一处。**禁止**绕过访问点直接 memcpy 含引用叶子的 blob 而不过屏障——
   纯 memcpy 只对"无引用叶子的 struct"（引用位图为空）合法，含引用叶子必须走带屏障的逐叶子/批量路径。
4. **装箱 struct** 是普通 `Value::Object`，走现有对象扫描 + 屏障。

> **落地要点**：`StructCopy` 不是无脑 `memcpy`——分两路：引用位图为空 → 纯 memcpy 快路径；含引用叶子 →
> 值字节 memcpy + 引用叶子逐个按**种类**正确复制。StructLayout 的引用位图是这条分流的依据。
>
> **引用位图必须带种类（2026-08-06 修正）**：引用叶子在 z42 是**托管**的、复制/析构语义按种类不同——
> 位图不能只标"引用在此"，要标 **arc-string vs gcref-object**：
> - **string（`Arc<str>`，16B）**：StructCopy 时 `Arc::clone`（引用计数 +1，raw memcpy 会漏计数→
>   double-free）；blob 析构时 `Arc::drop`（计数 −1）。
> - **object/array（`GcRef`，16B）**：位拷贝 + **GC 写屏障**（分代/并发追踪）；生命周期由 GC 管，无显式 drop。
> - **基元叶子**：raw 字节，无 clone/drop/屏障。
>
> 故只有**纯基元 struct（引用位图为空）**能走 raw memcpy 快路径；含任何引用叶子必须逐叶子按种类 clone/
> drop/屏障。这也要求 struct blob 有**确定性析构 hook**（帧退出/容器回收时对引用叶子逐个 drop/release）。

### ⚠️ Decision η：格式 bump（zbc/zpkg minor + 两阶段 nightly）

- **新 struct 指令**（Decision α）入 zbc → zbc minor bump。
- **struct 布局元数据**（offset/width）入 zpkg TypeDesc 供**跨包**消费 → zpkg minor bump。
- 触发 [version-bumping.md] 全 checklist（writer/reader 常量、fixture regen、golden hex）+
  [bootstrap-seed.md] **两阶段引入纪律**（support 先行、晚一 nightly 再 use；z42c/stdlib 源在新
  nightly 发布前**不得使用** struct 值语义新指令/布局）。
- ⚠️ **时机权衡**：新指令在 P1 就需要（局部 struct 复制），但跨包布局元数据到 P4 才必需。可否
  P1 只 bump zbc（指令）、P4 再 bump zpkg（布局），还是一次性——需在 P1 gate 定，避免多次 bump 反复
  踩两阶段纪律窗口。

### Decision θ：逃逸分析交互

- struct 恒内联，**不走** `ObjNew`→堆/`StackObject` arena。`IrEscapeAnalysis.z42` 对 struct 的 new 直接
  按内联布局处理，不参与"逃逸→栈 arena"判定。
- 引用类型（class）的 `StackObject`/`StackArray` 逃逸优化**完全不变**。
- 二者是**不同机制**：逃逸 arena = 引用类型的分配优化（仍引用语义）；struct 内联 = 值类型的语言语义。
  book 两页需交叉澄清，防混淆。

## P1 Gate 决策记录（2026-08-06 User "没问题" 通过）

阶段 6.5 gate 剩余 3 子决策，按以下默认采纳（User 未反对；如需调整回本节改）：

| 子决策 | 采纳 | 理由 |
|--------|------|------|
| **Decision α 操作数模型** | base + 字节区间 `(base,byte_offset,size)` + 帧局部字节区 | 唯一能表达字节打包内联的模型；`Value` 不加未装箱变体 |
| **格式 bump 时机** | **P1 只 bump zbc**（新 struct 指令）；**跨包字节布局元数据 zpkg bump 推迟 P4** | P1 同模块，不需跨包布局；提前 emit 跨包元数据是浪费且照样两阶段。每次 bump 各自走两阶段 nightly 纪律 |
| **P1 收敛面** | **P1 = 纯局部 struct**（局部/参数/返回/嵌套局部 lvalue）；class 的 struct 字段 + `struct[]` → P3 | 纯局部即可端到端验证「字节布局+复制+lvalue+GC 屏障」全机制（局部 struct 含引用叶子已触发 GC 屏障）；对象/数组存储是独立存储介质，P3 专攻 |

> **GC 读写屏障（User 强调）已在 P1 生效**：纯局部 struct 只要**含引用类型叶子**（如局部 struct 里放一个
> `List` 引用），其 blob 复制/叶子读写就必须过 Decision ζ 的读写屏障 + 引用位图扫描。故 P1 就要把 ζ 的
> 屏障机制做对，不是留到 P3。

## Architecture

```
编译器（z42c）                                 运行时（z42vm）
─────────────                                 ──────────────
StructLayout pass（新）                        TypeDesc: struct 布局(width/offsets)
  每 struct 类型 → {width, offsets}                    │
  嵌套递归展平 / 自含值字段报错                          ▼
       │                                       Frame.regs: Vec<Value>
       ▼                                         struct 值 = 连续区间 [base,base+width)
寄存器分配感知 width                                    │
  struct 局部/参数/返回 → 连续区间                       ▼
       │                                       struct 指令 dispatch:
       ▼                                         StructCopy: clone width 槽
lvalue codegen（3a）:                             StructFieldGet/Set: base+offset 区间
  obj.pt.x=5 → 叶子地址直写                         object slots 内联 struct 字段(P3)
  arr[i].x=5 → 扁平地址直写                          struct[] 扁平 backing(P3)
       │                                         boxing: struct 槽 → 堆 ScriptObject
       ▼                                         GC: 扫描区间内引用叶子(天然覆盖)
rvalue: 传参/返回/整体赋值 → StructCopy
装箱点: box/unbox 指令
```

## P1 实现子决策（2026-08-06 探索后定；⚠️ = 落 zbc/wire 前请 User 复核）

探索发现运行时**当前无任何字节布局概念**（`ScriptObject.slots: Box<[Value]>` 按 slot 序号、int 统一
`Value::I64`），编译器**无基元字节大小表**、`Z42ClassType` **不携带 struct-ness**。故以下为新增：

| # | 子决策 | 采纳解 |
|---|--------|--------|
| **1 ⚠️** | 布局数据落哪 | 访问指令（StructCopy/FieldGet/SetPrim）把 byte_offset/size 作为**立即数烘焙**（运行时无需查表）；**GC 引用位图 + struct 总 size 落 zbc TYPE section**（运行时 GC 扫描必需）。跨包布局传播 → zpkg（P4）。故 P1 bump zbc、TYPE section 加"struct 引用位图 + size"字段 |
| **2 ⚠️** | 基元字节大小/对齐 + 引用叶子表示（**2026-08-06 按实测 VM 表示修正**） | **基元**：`i8/u8/bool=1`、`i16/u16=2`、`i32/u32/f32/char=4`、`i64/u64/f64=8`（纯字节，可 memcpy）。char=4（z42 `Value::Char` 是 4B Unicode 标量，非 C#/Java 的 2B 变长 UTF-16 码元）。**引用叶子**：object/array/string/interface/func = **16B 托管句柄**（非 8B！`GcRef`=ptr8+generation4 对齐到 16、[refs.rs:66](../../../../src/runtime/src/gc/refs.rs#L66)；`Arc<str>`=胖指针 16B）——**不可 raw memcpy**。对齐：基元自然对齐、引用 8 对齐；struct 对齐=最大成员对齐，size 向上取整。**不逼 8B**（去 generation 丢 use-after-free 安全 / 句柄表是更大改造），留 Deferred |
| **3**（2026-08-07 改 A→B） | struct-ness 查询 | **方案 B**（实现时定）：`Z42ClassType.IsStruct` 字段，`SymbolCollector._putClassStub` 从 `ClassDecl.Kind` 回填。比方案 A（IrGen ClassDecls map）更优——TypeChecker(诊断)+IrGen(codegen) 都要 struct-ness，放类型上人人可查。`StructLayout.BuildFromSymbols(SymbolTable)` 据此抽 struct 字段（OwnFieldNames=名/OwnFieldSpellings=类型名）。**record 暂不算 struct**（引用语义，其值性另议） |
| **4** | 自引用/递归 struct 报错 | 放 **Pass1**（`TypeChecker` 有 `DiagnosticBag`）；StructLayout 计算时维护"在途类型集"，struct 直接/间接含自身**值**字段 → 报新诊断码（E04xx 段空号，如 `E0416`）。布局结果缓存供 codegen 复用 |

> **byte-size 约定（#2）与"落 zbc 引用位图"（#1）是定 ABI/wire 的**——StructLayout.z42 的**计算**是编译期
> 的（先做不锁格式），但一旦这些 offset/bitmap 进 zbc TYPE section 就成 wire 契约。故实现顺序：先落
> StructLayout 计算 + 单测（编译期，可改），到"新指令 + TYPE section 落 zbc"步前请 User 复核 #1/#2，
> 再走 version-bumping.md checklist + 两阶段 nightly。

## Implementation Notes

- **寄存器分配**：现有分配器按"每临时 1 槽"——需扩展为"struct 临时占 width 连续槽"。这是 P1 的核心
  编译器改动，牵动 `FunctionEmitter` 的 reg 分配与 `ExprEmitter` 的临时管理。
- **默认值**：struct 局部进入作用域即**零初始化其区间**（各叶子默认：int→0、ref→null、嵌套 struct
  递归）。无 null 默认。
- **相等**：`==` 对 struct = 逐叶子值相等（引用叶子比引用/或递归——C# 默认 `ValueType.Equals` 逐字段；
  引用字段用引用相等）。加比较指令的 struct 区间分支。
- **P1 边界收敛**：P1 只做**局部/参数/返回**内联（同模块），**对象内联(P3)/数组(P3)/跨包(P4)** 后续。
  P1 期 class 的 struct 字段可暂**限制或退化**（如暂不支持 struct 字段，或 P1 只测纯局部 struct）——
  具体 P1 收敛面在 P1 spec 定，避免 P1 咬住全部。
- **`Value` size 不增**：不新增 Struct 变体（未装箱走区间）；装箱复用 `Value::Object`。
- **与旧 `field_set` stack-handle assert 无关**：那是引用类型逃逸不变量，struct 不涉。

## Testing Strategy

- **P1 Golden**：局部 struct `b=a;b.x=99`→a 不变；`f(s)` 传参副本；`return s` 副本；嵌套局部
  `line.a.x=3`（3a 原地）；默认值零初始化；引用叶子复制共享。
- **Codegen**：StructLayout 布局 + StructCopy/区间访问 IrDump 对比。
- **Rust 单测**：区间 clone 复制语义；GC 扫描覆盖 struct 区间引用叶子。
- **格式（P4）**：zbc/zpkg fixture regen + golden hex + 跨包 struct e2e（cross-zpkg）。
- **GREEN**：每阶段 `xtask test` 全 gate；**自举字节不动点**——z42c/stdlib 源在对应 nightly 前不使用
  struct 值语义（support-first）→ gen1==gen2 不破。
- **JIT**：interp 全绿后评估（当前 interp 优先，遵 workflow）。

## 与规则的关系

- **bootstrap-seed.md 两阶段纪律**：格式 bump（Decision η）必须 support 先行、晚一 nightly 再 use；
  z42c/stdlib 源不率先使用新指令/布局。P4 跨包为纪律关键点。
- **philosophy.md 根因修复 + 不做兼容**：从产出端（布局+表示）改对；pre-1.0 直接切、不留引用语义
  兼容路径。
- **code-organization.md 行数**：`StructLayout.z42` 等新文件 ≤300 软限；重构 `ExprEmitter`/`types.rs`
  若超限按拆分规则独立 refactor commit。
- **memory**：修复 [z42-structs-not-value-types]；解锁 [packed-primitive-arrays] 的 inline struct[]。
