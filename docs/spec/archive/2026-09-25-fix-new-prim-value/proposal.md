# proposal：fix-new-prim-value

## 一句话

`new int()` / `new bool()` / `new string()`（以及泛型里的 `new T()` 当 T 是基元）此前**不产出值，
而是分配一个空的包装类对象** —— 于是要么崩在不相干的地方，要么静默给出一个既非 `true` 也非
`false` 的 `bool`。把它们折成**与 `default(T)` 一致的零值**（`string` 例外，产出 `""`），
令「约束判定说基元可构造」与「真去构造」两边口径一致。

## 症状：两条路都坏，其中两个是静默的

`T make<T>() where T : new() { return new T(); }`

| 类型 | 直接写 `new X()` | 泛型 `make<X>()` |
|---|---|---|
| `int` / `long` | 崩 `Int32.ToString: arg 0 expected int, got Object(...)` | 崩 `__box_prim: expected integer value, got Object(...)` |
| `double` | 崩 `Double.ToString: arg 0 expected double` | 同上 |
| `char` | 崩 `Char.ToString: arg 0 expected char` | 同上 |
| `bool` | 🔴 **静默错值** | 🔴 崩 `BrCond expects bool, got Object(...)` |
| `string` | 🔴 **静默坏值** | 🔴 **静默坏值** |

**`bool` 那条最坏 —— 它产出的东西既不是真也不是假**：

```z42
bool b = new bool();
if (b) { … }        // 走**真**分支
b == false          // false
b == true           // false        ← 三条互相矛盾，零诊断
```

`string` 那条同形：`new string()` 得到的对象 `== null` 是 false、`== ""` 也是 false，
一碰 `.Length` 才崩 `__str_length: arg 0 expected string`。

> ⚠️ **记忆/旧记录把这条记成「泛型约束 `new T()` 的问题」，那个边界是错的**：直接写
> `new int()` 一样坏。泛型只是让它更容易被撞见（`where T : new()` 明写着「基元满足」）。

## 契约早就在，只是构造侧没实现

三处已经说好了基元的零值是什么，且**都是对的**：

| 出处 | 说法 |
|---|---|
| `default(int/bool/char/double)` | 实测 `0` / `false` / `'\0'` / `0` —— 全对 |
| `ConstraintChecker._hasNoArgCtor` | `if (cls.IsScalarType()) { return true; }` + 注释「基元恒满足（同运行期）」 |
| 运行期 `generics.rs::validate_type_arg_constraint` | 同款放行 |

所以本 change **不引入新语义**，只是把「构造一个基元」实现成它早已被承诺的样子。对齐 C#：
`new int()` ≡ `default(int)` ≡ `0`。

## 根因：两条发射路径都把基元当普通类

- **直接形态**（编译期类型已知）：`ConstructTyper._bindNew` 不看类型是不是基元，径直走 ctor 键 →
  发 `obj_new int int.int()` → VM 按 `Std.Int32` 的 TypeDesc alloc 一个 0 字段的 ScriptObject。
- **泛型形态**（类型运行期才知）：`CallEmitter._emitNew` 发 `MethodTypeArgInsn` + builtin
  `__activator_create` → `builtin_activator_create` 拿到 `Std.Int32` 的**真句柄**照常 alloc。
  它开头那句 `bail!("... type has no runtime handle (primitive/array/synthetic?)")` 本以为基元
  没有句柄 —— 但 `Std.Int32` 是真 struct 类型、句柄一直在，那道门从不触发。

## 改动（两处，各管一条路）

### ① 编译期：`ConstructTyper._bindNew` 折成常量

类型解析完之后、进 ctor 键之前：

- `t.IsScalarType()`（`PrimCode` 0–11：`sbyte`…`char`）→ 返回 `BoundDefault(t, -1)`，
  **与 `default(t)` 完全同一个 Bound 节点** ⇒ 零值由构造保证一致，不会两处各写一份。
- `t` 是 `string`（`PrimCode` 12）→ 返回 `BoundLitStr("")`（见下「string 的裁决」）。
- 覆盖 target-typed `new()`：判定放在 `n.Type == null` / `!= null` 两支**汇合之后**，
  于是 `int x = new();` 同样命中。

🔴 **必须同时拦「带实参」的形态**：`new int(5)` 今天崩在运行期
（`MissingSymbolException: constructor Std.Int32.Int32`）。若只加折叠而不拦它，`new int(5)`
会**静默变成 0** —— 比现状更坏。故 `ArgCount != 0` 时报 **E0426**（`CtorArgMismatch`，已有码）
并照常折成零值（错误路径不再级联）。

⭐ **这条不是假想**：全仓唯二提到 `new <基元>(…)` 的地方，是 `z42.compression` 里两段
**踩坑注释**（`Tar.z42:411` / `Zip.z42:404`）——

> `new string(chars)` returned Null for empty char arrays（2026-05-26）
> `new string(chars)` returns a GcRef-wrapped namespace-local `Std.Archive.string` type
> rather than the primitive `Std.String`, breaking downstream CharAt / Length / Split（2026-05-27）

两次都**绕开**（改用 `String.FromChars`）而没有修根因 —— 根因正是本 change 这一条：
`new <基元>(…)` 走普通 ctor 路径、把基元当普通类。

⚠️ **但那两段注释描述的是 2026-05 的行为，今天已经不同**（实测：`new string(cs)` 抛可 catch 的
`Std.MissingSymbolException: constructor Std.String.String ...`，不再静默给 Null / 假对象）。
所以 E0426 的收益是**把运行期异常挪到编译期**，不是「堵一个静默洞」；真正非加不可的理由仍是
上一段那条 —— 不加的话折叠会让 `new int(5)` 静默变成 `0`。诊断消息里带上 `String.FromChars`
的指引（同 E0480「告诉你该怎么写」的做法）。

### ② 运行期：`builtin_activator_create` 认基元包装类

`Activator.CreateInstance`/`new T()` 拿到的 `Std.Type` 若指向基元包装类（`Std.Int32` …
`Std.Char`）→ 直接返回 `default_value_for(td.name)`（该函数**已经**认 FQ 包装名，
`fix-type-reflection-names` 加的）；`Std.String` → `Value::Str("")`。

这一处同时修好了反射入口 `Activator.CreateInstance(typeof(int))` —— 同一个根因，两个入口。

## `string` 的裁决（User，2026-09-24）

C# 里 `string` 没有无参 ctor：`new string()` 是 CS1729，`where T : new()` 也拒绝 string（CS0310）。
**z42 不照抄**，理由是 z42 自己的规则已经说了另一件事：`_hasNoArgCtor` 明写「**完全没有显式
ctor 也算满足（默认构造）**」，而 `Std.String` 正好没有声明实例 ctor ⇒ 按现行规则
`new string()` 合法。既然合法，它就必须产出一个能用的值。

⇒ **`new string()` == `""`**（`Length == 0`、`== ""` 为真）。

⚠️ **刻意与 `default(string)` 不同**（后者是 `null`）：`default` 是「没有值」，`new` 是
「构造一个」。两者在引用类型上本就该分开 —— 与 C# 的 `default(string)==null` 一致，
只是 z42 额外允许了 `new string()`。

**没有选「对齐 C# 报错」**：那要新取一个诊断码，还要同时改**编译期与运行期两份**约束判定
（`_hasNoArgCtor` + `generics.rs`）—— 两份口径必须一致，是 `where T : struct` 那条
（编译期与运行期各判各的）踩过的坑。范围明显更大，收益只是拒绝一个没人写的写法。

## 边界

- **`object`**：`new object()` 不在本 change（它今天产出一个非 null 对象，形态上没错；
  `o.GetType()` 报 `VCall: function Object.GetType not found` 是**另一条**缺口）。
- **用户 struct / class**：一行不改（走原 ctor 路径）。
- `new T()` 当 T 是**类级**型参：本 change 不碰（`_bindNew` 只处理方法级 `MethodParamIndexOf`；
  类级那条今天落 `_chkTypeRef`，与基元无关）。

## 无格式 bump

- ①是绑定期折叠，发的是既有的 `const.*` 指令；②是既有 builtin 的一个提前返回。
- **无新指令、无新 wire 字段、无新诊断码**（E0426 是既有码）。
- 产品代码里 `new <基元>()` 的出现次数：**0**（实测全仓 grep）⇒ 自举字节不受影响。
