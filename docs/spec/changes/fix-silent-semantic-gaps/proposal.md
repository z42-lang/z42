# Proposal: 修「编译通过但行为静默错误」的六条缺口

> Status: **DRAFT**（2026-09-17；等 User 裁决分期与取舍后进 IMPL）
> 分类：lang（语义 / 诊断）+ ir（新 opcode 发射）→ **走规范先行流程**
> 子系统：compiler（`z42c.semantics` / `z42.ir`）· 少量 runtime 核实
> 来源：三书重构批 3 的字段级核实副产品，见
> [batch3-verification.md](../restructure-docs-three-books/batch3-verification.md) 附录 A

## Why

批 3 在把 `docs/design/language/` 编入参考手册时，对 144 条技术断言做了字段级核实，
并**用 `./.z42/z42 run` 写探针实跑**（批 1/2 没做这一步）。结果落空 60 条，
副产品是 **30 条实测实现缺口**。

其中六条的性质与其余不同：**它们不是「缺功能」，而是「编译通过、行为错误、无任何诊断」。**

缺功能的代价是用户写不出来——他会立刻发现，然后绕行。
静默错误的代价是用户**以为写对了**——错误会一直躺在生产代码里，直到某天以别的形式爆出来。
这六条里有两条（三种写法）会让**写入凭空消失**，两条会让常见的 C# 惯用法在运行期崩，
一条让值语义静默失效，一条让重载调用莫名找不到。

**这就是本变更只挑这六条的判据：静默错误优先于缺功能。**

其余 24 条（`E0424` 等 54 条零发射点死码、`new int[n][]` 不解析、字符串插值格式说明符被丢弃、
闭包栈分配已失效等）留在 `batch3-verification.md` 附录里，按需另开 change。

## 六条缺口

> 全部实测复现过，探针输出见 `batch3-verification.md` 附录 A。

### 第一族 · `ref` 传址：写入静默丢失（缺口 1–2）

| 缺口 | 写法 | 应当 | 实测 |
|---|---|---|---|
| **1** | `void Inc(ref int x)` 调用写成 `Inc(v)`（漏写 `ref`） | 编译报错 | **编译通过，写入静默丢失** |
| **2** | `Inc(ref arr[0])` | `arr[0]` 被改 | **写入静默丢失**（打印 10 不是 11） |
| **2** | `Inc(ref h.f)` | `h.f` 被改 | **写入静默丢失**（打印 20 不是 21） |

缺口 2 的根因已定位得很清楚，而且**修法的地基已经在了**：

- `z42.ir` **只有 `LoadLocalAddrInstr`**（opcode `0xA0`）
- **运行时侧三条全有、且实现完整**：`0xA0` / `0xA1 LoadElemAddr` / `0xA2 LoadFieldAddr`
  （`src/runtime/src/metadata/zbc_reader/opcodes.rs:80-82`），执行在 `src/runtime/src/interp/exec_address.rs`
- `ExprEmitter.z42:110-123` 对任何 `BoundRefArg` 一律「先发射 inner，再取那个**临时寄存器**的地址」
  ⇒ 写回落在临时寄存器上，调用方看不到
- `RefKind::Array` / `RefKind::Field` 在运行时有定义，但**在 z42c 产物里没有生产者**

⇒ **VM 等着收这两条指令，编译器从来没发过。** 这是六条里修法最明确的一条。

### 第二族 · struct 值语义：一条语义错 + 两条运行期崩（缺口 3–5）

| 缺口 | 现象 | 实测 |
|---|---|---|
| **3** | 单字段 `struct` 仍是引用语义 | `struct One { int x; }` 的 `var b = a; b.x = 99;` → **`a.x` 也变 99**。根因：`StructLayout.IsBlobStruct` 要求「**多字段**」，单字段走不到 blob 值语义路径。两字段及以上实测正确 |
| **4** | struct 的 `static` / `static readonly` 字段读取即崩 | 抛 `struct-value handle used after its creating frame exited — value-struct lifetime unsound`。加不加 `readonly` 都一样 ⇒ `public static readonly Color White = ...` 这个 C# 常见惯用法在 z42 **完全用不了** |
| **5** | struct 上的自动属性崩 | `public int X { get; set; }` + 构造器赋值 → `struct ref leaf at byte offset 4294967295 not in type layout`（`u32::MAX`，明显是未初始化的 offset） |

缺口 4/5 尤其值得修，因为它们**只在运行期炸**：用户照 C# 习惯写完，编译器一声不响，跑起来才崩。

### 第三族 · 重载决议：子类实参不匹配基类形参（缺口 6）

| 缺口 | 现象 | 实测 |
|---|---|---|
| **6** | 重载集里不做「子类 → 用户基类 / 接口」适用性匹配 | 单签名 `One(A a)` 传子类 `D` ✅；但有 `F(A)` / `F(string)` **两个重载**时传 `D` → `E0401: no static method F on C`。实例方法同样。对比：**数值加宽**与 **`object` 形参**在重载集里**是**适用的，唯独用户类层次不是 |

> 编号说明：**六条缺口**，其中缺口 2「`ref` 到非局部左值」有两种表现（数组元素 / 对象字段），
> 上表拆成两行列出，共 7 行。下文一律用缺口编号 **1–6**。

## What Changes

**尚未定稿——三路根因调查进行中**，下列是待确认的修法方向，详见 [design.md](design.md)。

| 缺口 | 修法方向 | 性质 |
|---|---|---|
| 1 | 调用点修饰符校验（漏写 `ref` 报错） | 新诊断码；**会让现有代码变红**（含 stdlib，需先普查） |
| 2 | `z42.ir` 补 `LoadElemAddrInstr` / `LoadFieldAddrInstr`（`0xA1`/`0xA2`）+ `ExprEmitter` 按 inner 形态分流 | **发射新 opcode ⇒ 按 `version-bumping.md` 要 zbc bump**；VM 侧已就绪，自举安全（解码支持 2026-08-24 已进） |
| 3 | 放宽 `IsBlobStruct` 到单字段 | 语义变更；要普查 stdlib 里的单字段 struct 有没有代码依赖当前的引用语义 |
| 4 | struct 静态字段：装箱进堆 / 模块级 arena / **或先报清晰诊断** | arena 是 per-frame LIFO，与模块级生命周期有根本矛盾；可能短期只能先报诊断 |
| 5 | struct 自动属性的后备字段进 `StructLayout`，或先禁止并报诊断 | 同上，取舍待定 |
| 6 | 适用性判据加「子类 → 基类 / 接口」+ 同步择优规则 | 会**放宽**决议；风险是原本唯一确定的调用变成 `E0425` 歧义。跨包能否做全取决于 TSIG 里有无类层次信息 |

## 明确不做

- **不顺手修那 54 条零发射点死码**。它们是「缺检查」，不是静默错误；且接线每一条都要配发射点 + 回归测试，
  是独立工作量。清单在 `batch3-verification.md` 附录 B。
- **不改 `ref` / `out` / `in` 塌缩成同一标志这件事的全部后果**。`in` 只读、`out` 明确赋值分析、
  修饰符参与重载——这三条都是「缺检查」，且 `MangleKey` 加修饰符位可能牵动 zpkg TSIG 签名。
  本变更只做缺口 1（调用点校验），其余留待专门的 change。
- **不改 JIT**。三条取址指令在 JIT 里是 unsupported、回落解释器（`jit/translate/unsupported.rs:46-47`），
  这是既有设计；新发射 `0xA1`/`0xA2` 会让更多函数回落，性能影响需在 design 里评估，但不在本变更修。

## 待 User 裁决

1. **分期**：六条要不要一次做完？我的倾向是**至少拆三个 PR**（按三族），因为三族的爆炸半径完全不同：
   第一族动 IR 格式、第二族动 struct 布局语义、第三族动重载决议。混在一个 PR 里不可评审、不可回滚。
2. **缺口 4/5 的取舍**：真修（让 struct 静态字段 / 自动属性能用）还是**先报清晰的编译期诊断**？
   后者工作量小一个量级，且立刻把「运行期神秘崩溃」变成「编译期一句话说清」。
3. **缺口 1 的严格度**：漏写 `ref` 直接报错会让现有代码变红。要不要先发**警告**一个 nightly 再转错误？
4. **缺口 6 要不要现在做**：它是放宽而非收紧，风险形态与其余五条相反（可能引入新的歧义报错）。
