# JSON serde 的反射实现

> 代码：`src/libraries/z42.json/src/{JsonSerializer,JsonBinder,JsonMember,JsonReflect}.z42`、
> `src/runtime/src/corelib/reflection/attributes.rs`
>
> 公开 API（`Serialize` / `Deserialize<T>` / `[JsonProperty]` / `[JsonIgnore]` / 类型覆盖）见参考手册
> [Std.Json](https://z42-lang.github.io/z42/reference/stdlib/json.html)。本页写**这套东西怎么跑起来的**。
>
> DOM 层（`JsonValue` 的 parse / stringify）不在本页范围——serde 只是站在 DOM 之上的一层反射绑定。

serde 的反射计划限定在一次顶层操作内。成员和构造器按实际 Type 分别复用，加载器版本变化后重新解析。
若发现用户特性工厂，成员继续逐对象解析，保留特性构造与成员访问的顺序。集合在遍历内绑定方法并复用调用参数数组。

## 1. 分派轴：目标 Type，而非 JSON kind

反序列化核 `JsonBinder.FromJson(Type t, JsonValue v)` 以 **`t.FullName`** 为主分派轴：

```
FromJson(t, v):
  v.IsNull()                            → null
  fn == "Std.Int32"                     → (int)v.AsLong()
  fn == "Std.Int64"                     → v.AsLong()
  fn == "Std.Double"                    → v.AsDouble()      # AsDouble 内部容 long→double 提升
  fn == "Std.Boolean"                   → v.AsBool()
  fn == "Std.String"                    → v.AsString()
  t.IsArray                             → _fromArray  （反射建 elem[]，逐元素递归）
  baseFn == "Std.Collections.List"      → _fromList   （反射建 List<T>，逐元素递归）
  baseFn == "Std.Collections.Dictionary"→ _fromDict   （键必须是 string）
  else                                  → _fromObject （构造 + 成员绑定）
```

**为什么不按 JSON 值的 kind 分派**：JSON number 在 DOM 层统一落成 `long` 或 `double`，但目标成员可能
是 `int` / `long` / `double`。按 kind 分派拿不到「该 coerce 成什么」这个信息；按目标类型分派才能正确
窄化。序列化方向 `_toJson` 也按运行期类型分派。标量先通过泛型目标的 `is T` 判别，编译器在同包的具体调用点
把目标代换成对应的类型。裸整数按 Int32 路径处理，带精确类型的 Int64 盒保留 64 位值。其余值进入反射路径。

**集合检测必须排在通用对象分派之前**：否则 `List<T>` 会被当普通对象遍历它的 `items` / `Count` 内部
字段，序列化出一堆实现细节。

**匹配用去实参的基名**：`typeof(List<int>).FullName` 是 `Std.Collections.List<Std.Int32>`（构造型泛型
的 `FullName` 含实参），所以集合检测把 `FullName` 截到首个 `<` 前再比。基元 / 字符串分派不受影响
（基元的 `FullName` 里没有 `<…>`）。

`FromJson` 里还留着 `fn == "int"` / `"long"` / `"string"` 这类旧关键字分支。反射的 `FieldType` /
`PropertyType` / `typeof(T)` / `GetType()` 现已统一吐 `Std.*` 包装类 FQ，这些分支是防御式冗余。

## 2. 成员模型：字段与属性统一

`JsonMembers.For(Type)` 把两种成员抹平成同一个 `JsonMember`：

| 来源 | 收集方式 | 过滤 |
|---|---|---|
| 字段 | `t.GetFields()` | `!IsStatic && IsPublic` |
| 属性 | `t.GetProperties()` | 按公开实例 getter 与 setter 分别筛选 |

`JsonMember` 里藏一个 `FieldInfo` 或一个 `PropertyInfo`（二者其一非 null），对外只暴露
`JsonKey` / `MemberType` / `CanRead` / `CanWrite` / `Get` / `Set`。序列化只看 `CanRead`，反序列化只看
`CanWrite`。公开的只读实例属性仅参与序列化。一般反射仍可查看属性及其访问器，JSON 使用公开实例资格。

键名 = `[JsonProperty].Name`，否则成员名；`[JsonIgnore]` 直接排除。继承字段沿基类链读取声明处的特性。
整数属性经反射写入时，VM 按 setter 的参数表示把整数盒转换为标量，使解释器与已编译的 JIT setter
看到同一种参数。

> **auto-property 脱糖成私有背后字段 `__prop_X`，而 `GetFields()` 有意排除它**
> （`corelib/reflection/fields.rs::is_backing_field`）。所以属性只能经 `GetProperties()` 看到——两条
> 路径不会把同一个属性收两遍。

## 3. 构造模型：无参优先，带参兜底

构造器枚举读取目标类型的声明清单，入口包和惰性加载包均按同一规则绑定。

```
ctors = t.GetConstructors()
有无参 ctor（或一个 ctor 都没有）
  → Activator.CreateInstance + 对每个可写成员按 JSON 键 SetValue
否则
  → 选参数最多的那个 ctor，按**参数名**映射 JSON 键（递归 FromJson 到参数类型）→ Invoke
  → 再补齐剩余的可写成员（覆盖 record / 只读 auto-prop 的场景）
```

构造器扫描一次收集每个签名，选中带参构造器后复用该参数数组。成员与参数绑定在 JSON 对象上各查一次键，
命中后直接读同一槽位。缺键、JSON null 和非对象的判断沿用原绑定契约。

「按参数名映射」是这套设计的关键取舍：它让 record 与只读 auto-property 免配置就能反序列化，代价是
**ctor 的参数名成了外部契约**——改参数名会静默改变能接受的 JSON 形状。

缺键的行为分三种，注意第三种：

| 情形 | 结果 |
|---|---|
| 缺 JSON 键、目标是普通成员 | 成员保持默认值（整个 `Set` 不执行） |
| 缺 JSON 键、目标是可选 ctor 参数 | 用 `p.DefaultValue` |
| **缺 JSON 键、目标是必填 ctor 参数** | `FromJson(pt, JsonValue.OfNull())` → **`null`**，不报错 |

第三种对引用类型合理，对值类型参数就是把 `null` 塞进一个 `int` 位置。

多余的 JSON 键被忽略；`Dictionary` 的键类型非 `string` → `JsonException`。

## 4. attribute 的格式载体：复用 `field_attributes`

属性在 zbc 的 TYPE 段里**没有独立实体**可挂 attribute（它脱糖成「背后字段 + `get_` / `set_` 方法」）。
serde 需要读属性上的 `[JsonProperty]` / `[JsonIgnore]`，实现方式是**把属性的 attr 挂到合成的背后字段
`__prop_X` 的 `.Attrs` 上**，复用早已存在的 `field_attributes` 格式——**零格式 bump**，因此不触发两代
自举那套复杂度。

三处配合：

1. **AttributeSynth**（`_processMembers`）补 `PropertyDecl` 分支（key 形如 `prop$<Class>$<Name>`）。
   缺了这一步，属性上的 attr 会因 `FactoryFunc == ""` 被 `_attrRefsFromList` 静默跳过。
2. **ClassDescBuilder** 合成 `__prop_X` 时把该属性的 attr-refs 写进 `ibf.Attrs`。
3. **运行期** `__property_custom_attributes` 收 `PropertyInfo.__getterQualified`（`"<Class>.get_<Name>"`），
   在 Rust 侧按最后一个 `.` 切开、剥 `get_` / `set_` 前缀得属性名 → `__prop_<Name>` → 查该类型的
   `field_attributes`。传 accessor-qualified 名而不是属性名，是为了避免在 z42 侧对跨包字段做字符串
   操作。

**这条路子的固有限制：计算属性（有 getter 方法体、无背后字段）无处挂 attr。** 于是
`[JsonIgnore]` / `[JsonProperty]` 加在计算属性上会被**静默忽略**，见 §6。

## 5. 反射底座

serde 用到的反射能力分两批：

**已下沉为公开反射 API**（`add-array-property-reflection-api`）——

- **`Std.Array`**（照搬 C# `System.Array`）：静态 `CreateInstance(Type, int)` / `CopyRange`，实例
  `GetValue(int)` / `SetValue(object value, int index)` / `Clone()` / `.Length` 字段。用于以运行期
  Type 建 `T[]` 并读写元素。`SetValue` 是 **value 在前、index 在后**，对齐 C#，与直觉相反。
- **`Std.Reflection.PropertyInfo`**：`GetCustomAttributes()` / `GetAttribute(Type)`（镜像 `FieldInfo`）。

**仍留在 `JsonReflect.z42` 的集合辅助**——它们封装的都是已公开的反射 API
（`GetGenericArguments` / `Activator` / `MethodInfo.Invoke` / `FieldInfo.GetValue`），本库**不自带
任何 extern**：

| 辅助 | 实现 |
|---|---|
| `GenericArg(t, idx)` | `t.GetGenericArguments()[idx]` |
| `ListCount` | 每次列表遍历反射读一次 `Count` 公开字段 |
| `BindMethod` | 按名称、完整参数类型、public 实例及非泛型形态绑定唯一方法；缺失或多义抛 `JsonException` |

集合循环之前绑定 `get_Item` / `Add` / `Keys` / `set_Item`，循环内仅更新同一个参数数组并 Invoke。
构造泛型的反射参数类型已经替换为实际类型实参。绑定及参数数组归当前集合遍历所有，递归进入子集合会建立
独立的局部状态，因此嵌套调用不会覆盖外层参数，也不会跨操作或加载上下文复用反射对象。
空列表无需绑定元素方法；空字典序列化仅绑定 `Keys`。对象成员、构造器的操作内类型计划仍待实现。

> 两处跨包反射 footgun，全库随处可见：① 跨包读 imported 反射成员的引用类型返回值必须显式 cast
> （`Type elem = (Type)t.GetElementType();`）；② 反射调用要用 local receiver。

## 6. 已知缺口

共同特征：**全部静默**——不报错，输出直接不对，或者错误推迟到离现场很远的地方才炸。

| 缺口 | 实测行为 |
|---|---|
| `char` 成员 | 序列化成 `{}`——`_toJson` 没有 `Std.Char` 分支，落到 `_objectToJson`，而 `Std.Char` 的 `GetFields()` / `GetProperties()` 都是 0 长 |
| 计算属性上的 `[JsonIgnore]` / `[JsonProperty]` | 完全不生效——`[JsonIgnore]` 的成员照样序列化，`[JsonProperty("ren")]` 照样用原名（§4 的载体限制） |
| 必填 ctor 参数缺键 | 传 `null` 进去（§3） |
| `float` 成员反序列化 | 静默变 `0`（下表） |
| `enum` 成员反序列化 | 造出空对象，**下一次读它**才炸（下表） |

探针：

```z42
public enum Color { Red, Green }
public class G { public double D; public float F; public char C; public Color Col; public int? N; }
public class C2 {
    public int X;
    [JsonIgnore] public int Computed { get { return this.X + 1; } }
    [JsonProperty("ren")] public int Renamed { get { return this.X + 2; } }
}
// G{D=1.5,F=2.5f,C='q',Col=Green,N=7} → {"D":1.5,"F":2.5,"C":{},"Col":1,"N":7}
// C2{X=10}                            → {"X":10,"Computed":11,"Renamed":12}
```

同一次探针也澄清了两条被记成 Deferred、序列化方向实际能用的——都是**装箱后运行期类型恰好落进已有分支**：

| 成员声明类型 | `GetValue` 装箱后的 `FullName` | 命中分支 | 输出 |
|---|---|---|---|
| `float` | `Std.Double` | `Std.Double` | `2.5` |
| `enum` | `Std.Int32`（枚举本身的 `FullName` 是 `Color`） | `Std.Int32` | 序号 `1` |
| `int?` | `Std.Int32` | `Std.Int32` | `7` |

这是巧合而非设计——`_toJson` 里没有任何一行提到 float / enum / nullable。**反序列化方向不对称，而且更糟**：
`FromJson` 按**声明类型**分派，拿到的是 `Color` / `Std.Single`，一个分支都不命中，落进 `_fromObject`。
同一份 JSON 喂回去：

| 成员 | 序列化出 | 反序列化回来 |
|---|---|---|
| `double D` | `1.5` | `1.5` ✅ |
| `int? N` | `7` | `7` ✅ |
| `float F` | `2.5` | **`0`**——静默丢值 |
| `Color Col` | `1` | 造出一个空 `ScriptObject`，等到**下一次读它**才炸 `Std.Exception: __box_prim: expected integer value, got Object(...)`，且异常文本直接吐出整个 `TypeDesc` 的 Rust `Debug` 串 |

所以 float / enum 在参考手册里仍应记为**不支持**：序列化「看起来通了」只会掩盖往返不闭合这件事。

## 7. 跨包泛型 `typeof(T)` 的 handle 解析

`Deserialize<T>` 经 `typeof(T)` 拿目标 Type 交给 `Activator.CreateInstance`，这要求该 Type 携带运行期
handle（`NativeData::TypeHandle`）。方法级 `typeof(T)` 在运行期读 `frame.method_type_args[i]` 的**名字**
再 `make_type_from_name` 解析。

问题出在跨包泛型静态调用（`z42.json` 里的 `Deserialize<UserType>` 经 imported null-receiver vcall 派发）：
类型实参非唯一限定时可能只带**短名**（`"Point"`），按 FQ 键查 `type_registry` 落空 → 退化成无 handle 的
synthetic Type → `Activator` 报 `no runtime handle`。

兜底在 `make_type_from_name`：无点短名 FQ 查找失败后，按**简单名**在已加载类型（entry module + lazy
loader）里找，唯一匹配才解析为真 handle；零个或多义则维持 synthetic，绝不误绑。字段路径另有一条相关
兜底：字段 type_tag 的基名是短名（`List` 而非 FQN），若该集合类型未被 `typeof` / `new` 触发加载则从已
加载类型里找不到 → 对**首字母大写的非基元类名**一次性 `force_load_all_packages()` 后再唯一匹配
（gated 大写让基元不触发，force-load 幂等）。

> 根因在编译期的跨包泛型实参限定，兜底放运行期是当前最小且安全的位置。

另一处同族的坑：字段 type_tag 用**源拼写**，逗号后带空格（`Dictionary<string, int>`，来自 z42c
`_typeSourceName` 的 `", "`），原生 split 会得到 `" int"`（前导空格）→ `make_type_from_name` 落空丢
handle。修法是每个实参 trim。`typeof` 名里没有空格，所以这个坑**只中 member-type 反射的多实参泛型**。
