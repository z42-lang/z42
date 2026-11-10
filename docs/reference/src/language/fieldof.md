# `fieldof` / `propertyof` —— 字段与属性引用表达式

> [`typeof(T)`](methodof.md#与-typeof-的对称性) 指代**类型**，[`methodof(X.M)`](methodof.md) 指代**方法**，
> 本页补齐 member-of 家族的另两员：`fieldof(X.F)` 指代**字段**、`propertyof(X.P)` 指代**属性**。
> 求值分别为 `Std.Reflection.FieldInfo` / `Std.Reflection.PropertyInfo`。

## 它解决什么

与 `methodof` 同一条痛点，换了成员族：想让 attribute 记录「关联哪个字段 / 属性」（序列化字段选择器、
ORM 列映射、校验规则挂点），只能塞字符串——字段改名**静默失效**，写错名字要到运行期才发现：

```z42
// 字符串方案：Total 改名后 attribute 还在、值再读不回来，零提示
[Column("Total")] class Sheet { }

// fieldof 方案：改名 / 删字段 → 引用点编译错误
[Column(fieldof(Row.Total))] class Sheet { }
```

## 语法

```z42
fieldof(Box.A)              // 实例字段
fieldof(Box.Port)           // 静态字段（IsStatic == true）
propertyof(Box.Name)        // 属性（auto / 计算属性均可）
fieldof(Derived.Tag)        // 继承字段可经派生类指代
```

括号内是 ``Type.Member``，**没有签名列表**——字段与属性没有重载，名字即身份。也因此比 `methodof`
还严格两样（都报 `E0508`）：

| 写法 | 为什么不接受 |
|---|---|
| `fieldof(Box<int>.Item)` | 泛型类**共享一份字段布局**，`Box<int>.Item` 与 `Box<string>.Item` 是同一个槽 |
| `fieldof(Box.A(int))` | 字段无重载；参数列表只会让人误以为有消歧这回事 |

跨族点名给出**正确的关键字**（`E0507` / `E0509`）：`fieldof(Box.Name)`（Name 是属性）会直接说
「Name is a property — write `propertyof(Box.Name)`」。

## 语义细则

### 名字解析：与字段访问同一口径

沿基链**派生优先**取第一个同名符号（own-wins 遮蔽语义）——`fieldof(D.V)` 与 ``d.V`` 看到的
永远是同一份。命中后不再往基类找另一份；找到了就会与访问语义分叉。

### 声明类与 `__qualified`

编译期定位**声明类**，运行期按 `<声明类全名>.<成员名>` 物化反射对象——`FieldInfo.__qualified`
据此解析字段 attribute 工厂（attribute 挂在声明类的包元数据里）。

> **跨包边界**：TSIG 导出把基类实例字段**展平**进派生类（连带自有字段表），跨包方向上没有
> 「谁声明」的信号——`fieldof(Leaf.Mark)`（Mark 继承自另一包的基类）以点名的 `Leaf` 落名。
> 这与 `typeof(Leaf).GetFields()` 对同一字段的报告口径一致；存储读写不受影响。跨包属性以
> `get_P`/`set_P` 访问器方法过界，`propertyof` 绑定按访问器回落，运行期按访问器派生
> `PropertyInfo`（与 `GetProperties()` 同一条路）。

### 指不了的成员（诚实的残余）

| 情形 | 为什么 |
|---|---|
| `const` 字段 | **编译期折叠、无运行期槽**——`GetFields()` 也从不列出它。引用常量值本身即可（`E0507` 会说明原因） |
| 索引器 | 没有源码级名字（`this[i]`）；访问器 `get_Item` 可经 `methodof` 按方法指代 |
| 运算符 / 转换 | 同 [`methodof` 的残余](methodof.md#指不了的方法诚实的残余)——根本没有名字 |

## 与家族其他成员的对称性

| | `typeof(T)` | `methodof(X.M(…))` | `fieldof(X.F)` | `propertyof(X.P)` |
|---|---|---|---|---|
| 指代 | 类型 | 方法 | 字段 | 属性 |
| 结果类型 | `Std.Type` | `Std.Reflection.MethodInfo` | `Std.Reflection.FieldInfo` | `Std.Reflection.PropertyInfo` |
| 解析时机 | 编译期 | 编译期（重载决议） | 编译期 | 编译期（+跨包访问器回落） |
| 签名列表 | — | 有（消歧重载） | **无**（无重载） | **无**（无重载） |
| 值相等 | 按 `FullName` | 按 FQ 方法名 | 按 `__qualified` | 按 `__qualified` |

值相等语义家族一致：`fieldof(X.F) == fieldof(X.F)` 为 `true`（`FieldInfo` / `PropertyInfo` 的
`==` / `Equals` 按 `__qualified` 比），对象身份不驻留（`ReferenceEquals` 为 `false`，同
`MethodInfo`）。

## attribute 里的用法

`fieldof` / `propertyof` 都在 attribute 实参白名单里（`E0500`，见[特性](attributes.md#定义与应用)）——
成员定位全在编译期完成，与方法引用同一理由。完整可运行示例见
`src/tests/attributes/fieldof_in_attribute.z42`。

## 稳定性 = 普通字段访问的稳定性

落到 IR 只剩一个串池索引（`<声明类>.<成员名>`）+ 一次 builtin 调用，不新增指令、不动 zbc 格式。
运行期 `__fieldof` / `__propertyof` 按名查类型描述符物化反射对象；名字解析不到是编译器缺陷
（响亮崩溃），不是用户错误。

## 诊断

| 码 | 场景 |
|---|---|
| `E0507` | `fieldof` 目标不存在 / 是属性（给出 `propertyof` 写法）/ 是 `const`（说明原因）。列出可用字段名 |
| `E0508` | 括号内形态错：缺 ``Type.Member`` / 带类型实参 / 带参数列表 |
| `E0509` | `propertyof` 目标不存在 / 是字段（给出 `fieldof` 写法）。列出可用属性名 |

诊断下划线落在**成员名本身**，不是整个表达式。

## 不在本特性范围内

- **`ctorof`**：构造器没有源码级名字；`typeof(X).GetConstructors()` 与 `Activator` 已覆盖该场景。
- **`eventof`**：反射层还没有 `EventInfo` 对象；事件字段可经 `fieldof` 指代（`FieldInfo.GetValue` 照常）。
  补 `EventInfo` 是独立特性，届时按同族口径立案。
- IDE 的 Find All References / rename 跟随：同 [`methodof`](methodof.md#不在本特性范围内)，依赖 LSP 里程碑。
