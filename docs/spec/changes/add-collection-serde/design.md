# Design: 集合类型 serde（List<T> + Dictionary<string,V>）

## Architecture
```
JsonSerializer._toJson(o, t)               JsonBinder.FromJson(t, v)
  基元 → OfLong/OfDouble/...                 基元 → (int)/(long)/...
  t.IsArray → _arrayToJson                   t.IsArray → _fromArray
  fn=="Std.Collections.List" → _listToJson   fn=="Std.Collections.List" → _fromList     ← 本 change
  fn=="Std.Collections.Dictionary"→_dictToJson  fn=="…Dictionary" → _fromDict           ← 本 change
  否则 → _objectToJson（遍历成员）            否则 → _fromObject（构造 + 成员）

反射底座（JsonReflect，全 z42.json-local，复用既有 native）：
  元素类型   t.GetGenericArguments()[i]
  构造       Activator.CreateInstance(t)          （非泛型，作用于构造泛型 Type）
  List 枚举  FieldInfo(Count).GetValue + MethodInfo(get_Item).Invoke(o,[i])
  List 填充  MethodInfo(Add).Invoke(o,[item])
  Dict 枚举  MethodInfo(Keys).Invoke(o,[]) → string[]；MethodInfo(get_Item).Invoke(o,[key])
  Dict 填充  MethodInfo(set_Item).Invoke(o,[key,val])
```

## Decisions

### Decision 1: 反射-only，无新 native（User 裁决）
**问题：** 枚举/构造集合用反射，还是加 native 辅助（如 array 的 `__array_*`）？
**决定：** **反射-only**（User）。检测靠 `FullName`，枚举/填充靠 `MethodInfo.Invoke`（G2 #249）+
`FieldInfo.GetValue`，构造靠 `Activator.CreateInstance(Type)`。**零 runtime/compiler 改动、零格式 bump**。
代价：反射调用比 native 慢、代码略verbose、硬编集合类名（`Std.Collections.List/Dictionary`）——可接受
（C# System.Text.Json 亦硬识别 List/Dictionary）。

### Decision 2: 覆盖 List<T> + Dictionary<string,V>（字符串键）
**问题：** Set / 任意键 Dict / 其它容器？
**决定：** 只 List<T> + `Dictionary<string,V>`（User）。字符串键 → 地道 JSON object。非字符串键（→
array-of-pairs）、Set（无 HashSet）、Queue/Stack 等留 Deferred（roadmap Backlog）。

### Decision 3: 检测轴 = FullName（构造泛型定义名）
**问题：** 如何识别「这是个 List/Dict」？
**决定：** 按 `Type.FullName`——构造泛型 `List<int>` 的 FullName 是定义名 `"Std.Collections.List"`
（已 explore 验证：实例运行期类型与 typeof 均为此），元素类型另经 `GetGenericArguments()`（返回 `[int]`
/ `[string,int]`，已验证可靠）。检测分支置于**基元/数组之后、通用对象之前**（否则 List 被当对象遍历
其 items/capacity/Count 内部字段 → 错误）。

### Decision 3.5: 两个小 runtime 反射修复（实施期发现，User 批准）
字段路径反序列化（`class Bag { List<int> Nums; }`）暴露两个通用反射缺口——**成员类型**（`f.FieldType`）
的构造泛型无法解析到 runtime handle 构造。逐一定位并修（均是通用反射正确性改进，非集合特有）：

1. **`split_generic_args` 未 trim**（真根因）：字段 type_tag 用**源拼写**（z42c `_typeSourceName`
   逗号后加 `", "`）→ `Dictionary<string, int>` split 得 `["string", " int"]`，`" int"` 前导空格 →
   `make_type_from_name(" int")` 落空 → 合成无 handle → dict 值构造 `Activator(" Point")` no-handle。
   **修**：split 后每实参 `.trim()`。typeof 名无空格（`_typeofArgName` 不加空格），故此坑只中 member-type
   反射的**多实参**泛型（Dict）；单实参 List 不中。
2. **dotless 短基名不 force-load**：字段 type_tag 基名是短名 `List`（非 FQN）。`typeof(List<int>)` 走 FQ
   `Std.Collections.List` → `try_lookup_type` **force-load** → handle；字段短名只经既有「已加载类型简单名
   匹配」兜底（add-json-serde），若 List 未被 `typeof`/`new` 触发加载则找不到 → 无 handle。**修**：无点
   **类名**（首字母大写、非基元别名）从已加载找不到时，一次性 `force_load_all_packages()` 再按简单名唯一
   匹配。gated on 大写首字母 → 基元（`int`/`bool`…）不触发 eager load；force-load 幂等（`remaining_declared`
   加载后清空）。**权衡**：首次遇到未加载类名的反射会 eager-load 全部包（一次性、有界）；这是「无 simple→FQN
   索引」下唯一能 force-load 的方式。

### Decision 4: 反射成员按名查找（GetMethod 不存在 → 遍历 GetMethods）
z42 反射无 `GetMethod(name)`，故 `JsonReflect` 封装「遍历 `GetMethods()` 按 Name 取首个匹配」的辅助
（`Add` / `get_Item` / `set_Item` / `Keys`）。`Count` 是**公开字段**（非属性）→ `GetFields()` +
`FieldInfo.GetValue`。缓存不做（M2 求正确；性能后续）。

## Implementation Notes
- **跨包 imported-type footgun**（同 add-json-serde）：z42.json 读反射返回引用类型（Type/MethodInfo/
  FieldInfo）须显式 cast + local receiver。集合辅助里逐处遵守。
- **元素装箱**：反射 `Add`/`get_Item` 的 args/返回是 `object[]`/`object`——基元元素装箱往返，与既有
  `__array_get`/`set` 装箱路径一致（`FromJson` 已产精确 boxed 值）。
- **Dict 键必为 string**：`GetGenericArguments()[0].FullName` 非 `Std.String` → M2 抛 JsonException
  「unsupported dict key type」（清晰报错，非静默）。
- **嵌套**：`_fromList` 对每元素调 `FromJson(elemType, …)`——若 elemType 又是 List/Dict/对象，递归天然
  成立（测试覆盖 `List<Point>` / `Dictionary<string,Point>`）。

## Testing Strategy
- 单元 [Test]（`xtask test stdlib z42.json`）：List<int>/List<对象>/空 List 往返；Dictionary<string,int>/
  <string,对象> 往返；集合作为对象字段 + 顶层 `Deserialize<List<int>>`（若 typeof(List<int>) 顶层可行）。
- **无格式 bump → 本地完整 GREEN**（单代自举 `xtask test` 全 stage + self-host 不动点）。
