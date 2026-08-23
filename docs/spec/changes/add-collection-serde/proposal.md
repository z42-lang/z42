# Proposal: 集合类型 serde（List<T> + Dictionary<string,V>）

## Why
serde M2（add-json-serde）覆盖基元 + 嵌套对象 + 定长数组 `T[]`，但**泛型容器**（`List<T>` /
`Dictionary<K,V>`）无法序列化——真实用户缺口（DTO 里的 `List<Order>` / `Dictionary<string,int>` 字段
现在直接落到「按对象序列化其内部字段」的错误路径）。本 change 把 serde 扩展到最常用的两种容器。

## What Changes
1. **序列化**：`List<T>` → JSON array；`Dictionary<string,V>` → JSON object（字符串键）。
2. **反序列化**：JSON array → `List<T>`（反射构造 + 反射 `Add`）；JSON object → `Dictionary<string,V>`。
3. **z42.json 反射-only 引擎**（User 裁决）——无新 native、无格式 bump：
   - 检测：成员/运行期类型 `FullName == "Std.Collections.List"` / `"Std.Collections.Dictionary"`。
   - 元素类型：`Type.GetGenericArguments()`（List→[T]，Dict→[string,V]）。
   - 构造：`Activator.CreateInstance(memberType)`（**非泛型**，作用于构造泛型 Type——不依赖 G3）。
   - 填充：反射 `Add`（List）/ indexer `set_Item`（Dict）经 `MethodInfo.Invoke`（G2 #249）。
   - 枚举（序列化）：List 反射读 `Count` 字段 + `get_Item(i)`；Dict 反射 `Keys()` + `get_Item(key)`。
4. **两个小 runtime 反射修复**（实施期发现，User 批准；字段路径反序列化所需，通用反射正确性改进）：
   - **`split_generic_args` TRIM**：字段/成员 type_tag 用源拼写、逗号后带空格（`Dictionary<string, int>`，
     见 z42c `_typeSourceName` 的 `", "`）→ 原生 split 得 `" int"`（前导空格）→ make_type_from_name 落空
     丢 handle。修 = 每个泛型实参 trim（typeof 名无空格，故此坑只中 member-type 反射的多实参泛型）。
   - **dotless 简单名 force-load 兜底**：构造泛型的字段 type_tag 基名是**短名**（`List`，非 FQN）；若该
     集合类型未被 `typeof`/`new` 触发加载，`make_type_from_name("List")` 从已加载类型找不到 → 合成丢
     handle。修 = 无点**类名**（首字母大写、非基元）从已加载找不到时，一次性 force-load 全部声明包再按
     简单名唯一匹配（gated on 大写首字母 → 基元不触发；force-load 幂等一次）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.json/src/JsonReflect.z42` | MODIFY | 加集合反射辅助（GetGenericArguments 取元素类型 / 反射 Count·get_Item·Add·Keys 的封装） |
| `src/libraries/z42.json/src/JsonSerializer.z42` | MODIFY | `_toJson` 加 List/Dict 分支（在 object 分支前）→ `_listToJson` / `_dictToJson` |
| `src/libraries/z42.json/src/JsonBinder.z42` | MODIFY | `FromJson` 加 List/Dict 分支 → `_fromList` / `_fromDict` |
| `src/libraries/z42.json/tests/serialize.z42` | MODIFY | List/Dict 序列化 [Test] |
| `src/libraries/z42.json/tests/deserialize.z42` | MODIFY | List/Dict 反序列化 [Test] + 往返 |
| `src/libraries/z42.json/README.md` | MODIFY | 功能索引：集合覆盖 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `split_generic_args` trim + dotless 简单名 force-load 兜底 + `resolve_dotless_simple` helper |
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | `force_load_all()`（force-load 全部未加载声明包） |
| `src/runtime/src/vm_context.rs` | MODIFY | `force_load_all_packages()` wrapper |
| `docs/book/src/stdlib/json-serde.md` | MODIFY | 机制页：集合 serde（检测 / 反射枚举·构造 / 两 runtime 修复） |
| `docs/roadmap.md` | MODIFY | Deferred `json-serde-future-collections` 标 List/Dict 已交付 |

**只读引用**：
- `src/libraries/z42.core/src/Collections/List.z42` / `Dictionary.z42`（API：Count/this[]/Add/Keys）
- `src/libraries/z42.core/src/Reflection/{MethodInfo,Activator}.z42`（Invoke / CreateInstance）
- `src/libraries/z42.json/src/JsonMember.z42`（成员遍历——集合作为成员类型的分派入口）

## Out of Scope
- `Dictionary<K,V>` 非字符串键（→ JSON array-of-pairs）→ Deferred（本 change 只做字符串键 → JSON object）。
- `Set`（无 `HashSet`；`SortedSet` 在 z42.collections）/ `Queue` / `Stack` / `LinkedList` → Deferred。
- 嵌套容器（`List<List<T>>` / `Dictionary<string,List<T>>`）——递归 FromJson 天然支持，但作为**验证项**列入测试；不额外设计。

## Open Questions
- [ ] 无（机制已 explore 验证：GetGenericArguments 可靠 + 反射构造/Add 往返通）。
