# Design: 反射式数组 + 属性 attribute 反射 API 下沉

## Architecture

```
Phase 1（公开 API，z42.core）              Phase 2（改用 + 删 extern，z42.json）
─────────────────────────────            ──────────────────────────────────────
Std.Array (Array.z42) — 照搬 C#           JsonBinder / JsonSerializer
  + static CreateInstance ── __array_create  NewArray  → Array.CreateInstance
  + inst   GetValue(int)  ── __array_get      ArrayGet  → ((Array)o).GetValue(i)
  + inst   SetValue(v,i)  ── __array_set★     ArraySet  → arr.SetValue(v, i)
  + Length（既有字段）      ── FieldGet 分支     ArrayLength→ ((Array)o).Length

Std.Reflection.PropertyInfo               JsonMember
  + __customAttributes ─── __property_        JsonReflect.PropAttr(p,t) → p.GetAttribute(t)
  + GetCustomAttributes    custom_attributes
  + GetAttribute                             JsonReflect.z42: 删 5 extern + PropAttr
                                             （保留 GenericArg / List* / Dict*）

runtime 改动（Decision 4）：★ 重排 builtin_array_set (array,value,index)；删死 native __array_length。
```

native 均已注册在 `src/runtime/src/corelib/{array,reflection}.rs`。为忠实照搬 C# 的 `SetValue(value,index)`
需重排 `builtin_array_set` 一处 + 删死的 `__array_length`（见 Decision 4）——**无格式 bump，self-host 字节
不动点仍成立**。

## Decisions

### Decision 1: Array 反射 API 忠实照搬 C# `System.Array`（实例 GetValue/SetValue + Length 属性 + 静态 CreateInstance）

**问题：** 反射数组访问用 C# 风格实例方法，还是 z42-local 静态方法？

**决定（User 定：参考 C#，C# 设计无问题即用）：** 忠实照搬 C# `System.Array` 形态：
- `public static extern Array CreateInstance(Type elementType, int length);`（C# 静态工厂）
- `public extern object GetValue(int index);`（C# 实例）
- `public extern void SetValue(object value, int index);`（C# 实例，**value 在前 index 在后**——C# 原生签名顺序）
- 长度用**既有 `Length` 字段**（C# 是 `arr.Length` 属性），不新增 `GetLength`。

**运行期已核实可行（三点验证）：**
1. **下转型 `(Array)o` 成立**：z42c `_emitCast` 把 `(Array)` 规范化为 `Std.Array` 发 `AsCastInstr`
   （`TypeOpEmitter.z42:71`）；VM `as_cast` 对 `Value::Array` 走 `is_array_isa("Std.Array")==true`
   （`exec_object.rs:446` / `exec_vcall.rs:121`）→ 原样返回数组值。反射持 `object` 的调用点
   `((Array)o).GetValue(i)` 因此成立。
2. **实例 `[Native]` 方法 this 落 `args[0]`**：既有 `__array_clone`（实例）即读 `args[0]=this`
   （`array.rs`）→ 实例 `GetValue(int)` 的 native 参数 `[this=array, index]` 与 `builtin_array_get`
   的 `(array, index)` 完全对齐，**GetValue / CreateInstance 无需改 native**。
3. **`.Length` 走 VM FieldGet 硬编码分支**（`exec_object.rs:222` `Value::Array => "Length" => len`），
   与静态类型无关，`((Array)o).Length` 成立。

**代价（本 change 不再是「零 runtime 改动」，见下 Decision 4）：** 仅 `SetValue(value, index)` 的
C# value-在前顺序与既有 `builtin_array_set` 的 `(array, index, value)` 冲突 → 需**重排该 native** 读法。

**为何不选 z42-local 静态（曾经的推荐 A）：** User 明确要求参考 C#；C# 实例形态已核实在 z42 完全可行，
仅需一处受控 native 重排。静态形态虽零 runtime 改动，但不 C#-idiomatic，弃之。

### Decision 4: `builtin_array_set` 重排 + 删死 native `__array_length`（受控 runtime 改动）

**问题：** C# `SetValue(object value, int index)` 作实例方法 → native 参数 `[this=array, value, index]`；
既有 `builtin_array_set` 读 `args[1]=index, args[2]=value` → 顺序错位。且 `Length` 改用字段后
`__array_length` native 无人调用。

**决定：**
- **重排 `builtin_array_set`**：改读 `args[1]=value, args[2]=index`（`corelib/array.rs`）。**安全**：
  `__array_set` 现仅 `JsonReflect.ArraySet`（本 change Phase 2 删）一个调用点；重排与「Phase 2 新调用点走
  实例 `SetValue(value,index)`」在同一 PR 内原子一致，无遗留旧序调用。
- **删死 native `__array_length`**（`builtin_array_length` fn + `corelib/mod.rs` 注册）：全仓仅
  `JsonReflect.ArrayLength`（Phase 2 删）引用；C# 用 `.Length` 属性取代 → 删之（philosophy 不留死代码）。
- **自举影响可控**：① 无 zbc/zpkg 格式变化 → **self-host 字节不动点仍成立**（z42c 产物字节不受 native 读法/
  注册表影响）；② bootstrap 期（建 z42c/stdlib）**从不运行期调用** `__array_set`/`__array_length`
  （z42c 用 IR `array_set_elem` 而非反射 native；反射 native 仅 serde 运行期用），故种子 VM 缺重排/缺删除
  **不影响 bootstrap**；③ 本地改 runtime 后须 `cargo build --release` + `cp` 同步 `.z42/bin/z42vm`
  （见 [[sync-seed-vm-after-rebase-adds-runtime-builtin]] / [[measure-before-optimizing-and-nohup-trap]]）。

### Decision 2: PropertyInfo attr 复用访问器 qualified，不加 VM 写入字段

**问题：** `FieldInfo` 的 attr 查找依赖 VM 写入的 `__qualified`（"<Class>.<Field>"）。PropertyInfo 无此字段
（只有 `__getterQualified`/`__setterQualified`）。要不要给 VM 加写 `PropertyInfo.__qualified`？

**决定：** 不加。`GetCustomAttributes()` 用 `this.__getterQualified ?? this.__setterQualified` 作为传给
`__property_custom_attributes` 的字符串——native 已实现「按最后一个点切，strip `get_`/`set_` 前缀 →
属性名 → backing field `__prop_<Name>`」（`reflection.rs:530-544`）。→ **零 VM 改动**，且不依赖种子无的字段，
自举无忧。这也正是既有 `JsonReflect.PropAttr` 的做法，逻辑原样并入 `PropertyInfo`。

`__customAttributes` 声明为**实例** extern（镜像 `FieldInfo`）：native `builtin_property_custom_attributes`
用 `args.iter().find_map(Value::Str)` **lenient 扫描**取字符串（`reflection.rs:523`），故 `this` 占 `args[0]`
不影响取值——实例调用安全。

### Decision 3: 文档落点——更新 json-serde.md（必做）+ reflection.md API 列表（迁移问题留 Open Question）

**问题：** 反射 SoT 现在 `docs/design/language/reflection.md`（旧位置），doc-system D2 规定 `docs/design/`
不再更新、知识上浮 book，但 book 尚无反射页。

**决定（推荐）：**
- **必做**：`docs/book/src/stdlib/json-serde.md` 的「反射底座的自举约束（JsonReflect 库自有 extern）」节
  改写为「已下沉为公开 API」——这是本 change 直接影响、且已在 book 的内容。
- **必做**：`reflection.md` 新增公开 API 列表补上 `Array` 反射静态 + `PropertyInfo` attr（作为过渡；
  避免 SoT 缺项）。
- **不做**（本 change 不承接）：新建完整 book 反射页的迁移工作量大，属独立 docs change。→ 若 User 要求
  顺带迁移，扩 Scope；否则记为后续。**这是 Open Question，6.5 请 User 定。**

## Implementation Notes

- **Array.z42 新增（照搬 C#；`Array` 类现约 40 行，加后仍远低于 200 行类限）**：
  ```
  [Native("__array_create")] public static extern Array  CreateInstance(Type elementType, int length);
  [Native("__array_get")]    public extern object GetValue(int index);
  [Native("__array_set")]    public extern void   SetValue(object value, int index);  // C# 顺序：value 先
  // 长度用既有 Length 字段，不新增方法。
  ```
- **runtime（Decision 4）**：
  - `corelib/array.rs` `builtin_array_set`：`let value = args.get(1); let i = args.get(2)`（原为
    `i=args.get(1), value=args.get(2)`），即改读 `(array, value, index)`。
  - `corelib/array.rs` 删 `builtin_array_length` fn；`corelib/mod.rs` 删 `("__array_length", …)` 注册行。
- **PropertyInfo.z42 新增**：`public Std.Attribute[] __attrCache;` + `[Native("__property_custom_attributes")]
  public extern Std.Attribute[] __customAttributes(string qualified);` + `GetCustomAttributes()`（缓存 +
  `__getterQualified ?? __setterQualified` 兜底空数组）+ `GetAttribute(Std.Type)`（按运行期类型 `FullName`
  **精确匹配**，镜像 `FieldInfo.GetAttribute`）。
- **Phase 2 调用点替换**（含下转型 hoist；6 处 + 1 helper 删除）：
  - `JsonBinder.z42:32` `object arr = JsonReflect.NewArray(elem, n)` → `Array arr = Array.CreateInstance(elem, n)`
  - `JsonBinder.z42:35` `JsonReflect.ArraySet(arr, i, v)` → `arr.SetValue(v, i)`（arr 已是 Array，无需 cast）
  - `JsonSerializer.z42:77,80` 序列化处 `o` 是 `object` → hoist `Array a = (Array)o;` 后
    `int n = a.Length;` / `a.GetValue(i)`
  - `JsonMember.z42:84,92` `JsonReflect.PropAttr(p, typeof(X))` → `p.GetAttribute(typeof(X))`
  - `JsonReflect.z42`：删 line 17-28（5 extern）+ line 35-48（`PropAttr` 辅助）。
  - z42.json 需 `using Std;`（Array）与 `using Std.Reflection;`（PropertyInfo）——JsonMember 已 using Reflection；
    JsonBinder/JsonSerializer 确认 `using Std;` 在位（Array 在 `Std`）。
- **跨包 imported 反射返回引用类型 footgun**：属性 attr 走 `p.GetAttribute(...)` 返回 `Attribute` →
  调用点已有 `Attribute a = ...` local receiver + 后续 cast，沿用现状。

## Testing Strategy

- **单元测试（NEW `reflection_api_downsink.z42`）**：
  - 反射数组 int/string roundtrip（CreateInstance→SetValue→GetValue/GetLength）覆盖 spec 场景 1-3。
  - 对编译期 `int[]` 以 `object` 反射读取（spec 场景 3）。
  - PropertyInfo attr：定义带 `[PropTag("x")]` 的属性类，`GetProperties()` 取 PropertyInfo →
    `GetAttribute` 命中 / 未标注返回 null / 计算属性空数组（spec 场景 4-7）。
- **回归**：`xtask test stdlib z42.json`（serde 行为不变，Phase 2 改用 + 删 extern 后仍全绿）。
- **改 runtime 后必做**：`cargo build --release` + `cp artifacts/build/runtime/release/z42vm .z42/bin/z42vm`
  （否则 warm `xtask test` 用旧 VM，SetValue 重排/GetValue 走不通 → 假红/假绿）。
- **完整 GREEN**：`xtask test`（含 `test compiler` 自举 5/5——本 change 零格式 bump，gen1==gen2 字节不动点
  验证 native 重排/删除不扰动自举产物）。
