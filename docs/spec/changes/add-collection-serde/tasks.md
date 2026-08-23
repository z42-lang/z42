# Tasks: 集合类型 serde（List<T> + Dictionary<string,V>）

> 状态：🟡 进行中 | 创建：2026-08-23

## 进度概览
- [x] 1. JsonReflect 集合反射辅助
- [x] 2. 序列化分支（_listToJson / _dictToJson）
- [x] 3. 反序列化分支（_fromList / _fromDict）
- [x] 3.5 两个小 runtime 反射修复（split_generic_args trim + dotless force-load 兜底，实施期发现·User 批准）
- [x] 4. 测试（serialize / deserialize，18+13 绿）
- [ ] 5. 文档同步（README / book / roadmap）
- [ ] 6. GREEN + 归档 + PR

## 阶段 3.5: runtime 反射修复（实施期扩展）
- [x] 3.5.1 `reflection.rs` `split_generic_args`：每泛型实参 `.trim()`（去源拼写 `", "` 的前导空格）
- [x] 3.5.2 `reflection.rs` `make_type_from_name` dotless 兜底：无点大写类名从已加载找不到 → `force_load_all_packages()` 再简单名唯一匹配；抽 `resolve_dotless_simple` helper
- [x] 3.5.3 `lazy_loader.rs` `force_load_all()` + `vm_context.rs` `force_load_all_packages()` wrapper

## 阶段 1: JsonReflect 集合反射辅助
- [ ] 1.1 `JsonReflect.z42`：`_findMethod(Type, name)`（遍历 GetMethods 按 Name）+ `_findField(Type,name)`
- [ ] 1.2 元素类型辅助：`GenericArg(Type, idx)`（GetGenericArguments，cast + local receiver footgun）
- [ ] 1.3 List 读写：`ListCount(o)` / `ListGet(o,i)` / `ListAdd(o,item)`（反射 Count 字段 + get_Item + Add）
- [ ] 1.4 Dict 读写：`DictKeys(o)` / `DictGet(o,key)` / `DictSet(o,key,val)`（反射 Keys + get_Item + set_Item）

## 阶段 2: 序列化
- [ ] 2.1 `JsonSerializer.z42`：`_toJson` 加 `fn=="Std.Collections.List"` → `_listToJson`（基元/数组分支后、对象前）
- [ ] 2.2 `_listToJson`：Count + ListGet 遍历 → JsonValue.OfArray + 递归 _toJson
- [ ] 2.3 `_dictToJson`：DictKeys + DictGet 遍历 → JsonValue.OfObject（字符串键，值递归）

## 阶段 3: 反序列化
- [ ] 3.1 `JsonBinder.z42`：`FromJson` 加 List/Dict 分支
- [ ] 3.2 `_fromList`：GenericArg(t,0)=elemT；Activator 构造；逐 JSON 元素 FromJson(elemT) + ListAdd
- [ ] 3.3 `_fromDict`：键类型须 Std.String（否则 JsonException）；GenericArg(t,1)=valT；构造；逐键 FromJson(valT) + DictSet

## 阶段 4: 测试
- [ ] 4.1 `tests/serialize.z42`：List<int>/List<对象>/空 List；Dictionary<string,int>/<string,对象>
- [ ] 4.2 `tests/deserialize.z42`：对应反序列化 + 往返 + 嵌套（List<Point> / Dictionary<string,Point>）+ 非字符串键报错

## 阶段 5: 文档同步
- [ ] 5.1 `z42.json/README.md` 功能索引：集合覆盖
- [ ] 5.2 `docs/book/src/stdlib/json-serde.md`：集合 serde 机制节
- [ ] 5.3 `docs/roadmap.md`：Deferred `json-serde-future-collections` 标 List/Dict 已交付

## 阶段 6: 验证 + 归档
- [ ] 6.1 `xtask test`：完整 GREEN（stdlib z42.json + self-host 不动点 5/5 + e2e）
- [ ] 6.2 spec scenarios 逐条覆盖确认
- [ ] 6.3 归档 + PR

## 备注
- 反射-only、无新 native / IR / 格式改动（User 裁决）。
- 已 explore 验证：GetGenericArguments 可靠（List<int> 实例/typeof 均 GA=[int]）；Activator.CreateInstance(typeof(List<int>)) + 反射 Add → Count=1 [0]=42。
- 跨包 imported-type footgun：反射返回引用类型逐处 cast + local receiver。
