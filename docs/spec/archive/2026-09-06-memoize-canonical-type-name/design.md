# Design: 类型规范名（Canon）改为每实例记忆 + 内建码投影

## 1. 内建码（`PrimModel.Code`）

码序固定为 **i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 bool char string object**（0..13），
`-1` = 非内建。四张投影表按此序对齐：

| 表 | 用途 | 非内建时 |
|---|---|---|
| `_fq` | canonical → FQ（`Std.Int32`…） | `""` |
| `_wrap` | canonical → 包装类短名（`Int32`…） | 原样入参 `n` |
| `_kw` | canonical → 反射/typeof 名（`i32`→`int`、`f64`→`double`，其余等于 canon） | canon 原样 |
| `_tag` | canonical → `IrType` 标签 | `IrType.Ref` |

`Code` 的入参**必须是 `Canon` 的结果**（短名）。定位规则：

- `L == 3`：首字符 `f` 单独处理（`f32`→8、`f64`→9）；`i`/`u` 取基址 0/4，再按后两字符
  `16`/`32`/`64` 加 1/2/3。**零字符串比较。**
- `L == 2`：次字符必须是 `8`，首字符 `i`→0、`u`→4。**零字符串比较。**
- `L == 4` / `L == 6`：先按首字符预筛（`b`/`c`、`s`/`o`），命中才落**一次**完整字符串比较。
  预筛不能省掉那次完整比较——用户类名 `"book"` / `"strict"` 会撞首字符。
- 其余长度：直接 `-1`。

八个投影统一写成 `int c = PrimModel.Code(PrimModel.Canon(n));` + 表索引 / 区间判定：

| 投影 | 判定 |
|---|---|
| `IsBuiltin` | `c >= 0` |
| `IsScalarValue` | `0 <= c <= 11`（整数族 + f32/f64 + bool + char；string/object 是引用类型） |
| `IsInteger` | `0 <= c <= 7` |
| `IsNumeric` | `0 <= c <= 9 或 c == 11`（整数族 + f32/f64 + char） |

`IsNumeric` 由此从**两次 Canon**（自己一次 + 经 `IsInteger` 一次）降到一次；
`SurfaceName` 同理（原来 `IsBuiltin` + `Keyword` 各一次）。

## 2. 每实例记忆（`Z42Type.CanonName` / `PrimCode`）

```
Z42Type      : virtual CanonName() = PrimModel.Canon(Name())
               virtual PrimCode()  = PrimModel.Code(CanonName())
Z42ClassType : override，惰性字段 _canon / _code
```

**为什么只在 `Z42ClassType` 缓存**：`Z42ArrayType` / `Z42FuncType` / `Z42InstantiatedType`
的 `Name()` 是**现拼**的（`Elem.Name() + "[]"`、`"Func<" + … + ">"`），缓存 canon 等于把拼串
成本换成一次性的——语义仍然等价，但收益都在 `Z42ClassType`（profile 里 44 处调用点几乎全是它），
先不铺开。

**缓存恒有效的前提**：`Z42ClassType._name` 是 `private`、只在构造函数赋值，全仓无 setter
（`grep _name` 已核）。

**哨兵**：`_canon == ""` 表示未算（`Canon` 对非空名恒返回非空；名为空的退化类型只是每次重算，
无正确性影响）；`_code == -2` 表示未算（`-1` 是「非内建」这个**合法结果**，不能兼作哨兵）。

**`Z42ClassType.Builtin` 灌种**：profile 里 `IsScalarValue` 的 64% 来自这里。改成
`_canon = Canon(name)` → `_code = Code(_canon)` → `IsStruct = 0 <= _code <= 11`，
三个问题共用一次扫描，且后续对该实例的 `CanonName` / `PrimCode` 直接命中。

## 3. `IsAssignableTo` 的等价改写

原逻辑（一次调用最多 6 次 Canon）：

```
if (IsBuiltin(this._name) && Canon(this._name) == Canon(other.Name())) return true;
if (IsScalarValue(this._name) && IsScalarValue(other.Name()))
    return CanWiden(Keyword(this._name), Keyword(other.Name()));
```

改写后（两个名字各一次，且都走记忆）：

```
int mc = this.PrimCode();
if (mc >= 0 && this.CanonName() == other.CanonName()) return true;
int oc = other.PrimCode();
if (mc >= 0 && mc <= 11 && oc >= 0 && oc <= 11)
    return CanWiden(KeywordOfCode(mc), KeywordOfCode(oc));
```

逐条同义：`IsBuiltin(x) ≡ code(x) >= 0`；`IsScalarValue(x) ≡ 0 <= code(x) <= 11`；
`Keyword(x)` 在 `code >= 0` 时 ≡ `KeywordOfCode(code)`。
`other` 若不是 `Z42ClassType`，`other.PrimCode()` 走基类现算 ≡ 原来的 `PrimModel.X(other.Name())`。

## 4. 转发器的代价

`Z42Type.Canon` 是 `return PrimModel.Canon(n);` 一行的纯转发器，profile 里**自身**占 **0.53%**
——一整个 z42 调用帧（含 `VmFrame` 的两个 `Arc<str>` 克隆）就为了转个手。删掉，调用点直呼
`PrimModel.Canon`。同样地，实现中一度加过的 `PrimModel.CodeOf(n) = Code(Canon(n))` 也内联掉。

> 一般规律：在解释执行的编译器里，**一层纯转发 ≈ 0.5% 全负载**。加薄封装前先想清楚它是否值这个价。

## 5. 实测（同机交错 A/B，同一份输入源码，两个 driver 交替各 4 次）

| | 指令数 | 墙钟 | 峰值 RSS |
|---|---|---|---|
| base（`813a8c13`） | 66.210 G | 5.485 s | 993.3 MB |
| 本变更 | **63.308 G (−4.38%)** | **5.218 s (−4.87%)** | **901.4 MB (−9.25%)** |

跨运行离散度：指令 0.04%、墙钟 0.5%、RSS 0.01%。

**RSS 为什么也降**：默认从不 GC ⇒ RSS ≈ 总分配量。`Canon` 在剥 `?` / `Std.` 时会
`Substring` 出新串，调用次数砍掉一大截就等量砍掉了这些临时串。

**分层收益**（各自单独测过，基线 `40d274c2`，与上表同源）：
- 只做 §1（码 + 投影表，不改调用点）：指令 −0.37%、RSS −8.2%。
  ⇒ **那条 14 次字符串 `==` 的链本身不贵**（`==` 是内建、不进 z42 帧）；
  真正省下的是 `IsNumeric` / `SurfaceName` 的第二次 Canon 及其临时串（RSS 的那一大块）。
- 加 §2 §3（每实例记忆 + 调用点改写）：累计 −4.09%。
- 再加 §4（删两层转发器）：累计 **−4.44%**。
