# Proposal: JSON 对象序列化 / 反序列化（serde M2）

## Why

`z42.json` 目前只有 DOM 层（`JsonValue` / `Parse` / `Stringify`），无「对象 ↔ JSON」序列化能力。
兑现 roadmap 0.4.x **L 流招牌** `Deserialize<T>` 泛型 serde + 反向 `Serialize`。G2（#249）已备齐所需
反射原语（`typeof(T)` 方法级泛型、`ConstructorInfo.Invoke`、`Type.GetConstructors`、`Activator`、
`FieldInfo`/`PropertyInfo` Get/Set）。不做则 z42 无法在无手写 DOM 代码的情况下把用户对象与 JSON 互转。

## What Changes

1. 新 `Std.Json.JsonSerializer`：`Serialize(object)` / `SerializePretty(object)` /
   `Deserialize<T>(string)` / `Deserialize(Type, string)`。
2. 反射引擎：非泛型核 `_fromJson(Type, JsonValue)`（按**目标 Type** 递归分派）+ 序列化
   `_toJson(object)` + `JsonMember`（字段 + 属性统一抽象）。
3. `[JsonProperty("name")]` / `[JsonIgnore]` 特性（**字段 + auto-property 均生效**；计算属性不覆盖，
   见 design Decision 2 限制）。
4. **无格式 bump**：属性 attr 复用既有 `field_attributes` 载体，挂合成的 `__prop_<Name>` 背后字段
   （最初计划新增 `property_attributes` 段作格式 bump，因 CI 两代自举复杂度回退，见 design Decision 2）。
5. `System.Array` 反射 4 静态（`CreateInstance` / `GetValue` / `SetValue` / `GetLength`）——反射式
   建/读写 `T[]`（元素类型只以运行期 `Type` 已知）。

## Scope（允许改动的文件）

### runtime（Rust VM）

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/runtime/src/corelib/array.rs` | MODIFY | 4 个反射 array builtin |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `__property_custom_attributes` builtin（剥 get_/set_ → `__prop_<Name>` 查 field_attributes）+ `make_type_from_name` 无点短名兜底（跨包泛型 typeof handle 修复） |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 注册 5 个 native |

> **无格式 bump**：`bytecode.rs` / `loader.rs` / `types.rs` / `zbc_reader.rs` / `ZbcFormat.z42` /
> `ZbcWriter.z42` / `ZbcReader.z42` / `ZpkgWriter.z42` / `IrModule.z42` **均不改**（最初 format-bump 尝试
> 已回退，见 design Decision 2）。

### compiler（z42c.semantics）

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | 合成 `__prop_X` 背后字段时把属性 attrs 填入其 `.Attrs`（既有 field_attributes 格式） |
| `src/compiler/z42c.semantics/src/AttributeSynth.z42` | MODIFY | `_processMembers` 补 `PropertyDecl` 分支——为属性 store-meta attr 合成工厂 + 记 FactoryFunc |

### stdlib z42.core

> **实施期调整（自举约束，2026-08-22）**：原计划给 `Std.Array` 加反射静态、给 `PropertyInfo`
> 加 attr API。实测**冷启动 / download-bootstrap gen0 用种子 stdlib 编 z42.json** → z42.json 引用这些
> **新 z42.core API** 会 `E0401`（axis② stdlib API 面约束——种子 prelude 无新成员）。故**回退 z42.core
> 改动**，把反射底座（array native + property-attr native）声明为 **z42.json 自有 extern**
> （`JsonReflect.z42`），仅运行期由新 VM 解析，编译期不依赖种子 prelude。所有 serde **能力不变**；
> 公开 `Std.Array`/`PropertyInfo` 反射 API 作为本 nightly 发布后的 follow-up。

### stdlib z42.json

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/libraries/z42.json/src/JsonSerializer.z42` | NEW | 公开 API + `_toJson` 序列化 |
| `src/libraries/z42.json/src/JsonBinder.z42` | NEW | `_fromJson` 反序列化 + ctor 绑定 + 数值 coercion |
| `src/libraries/z42.json/src/JsonMember.z42` | NEW | 字段 + 属性统一成员抽象（键名解析） |
| `src/libraries/z42.json/src/JsonReflect.z42` | NEW | 反射底座——array native + property-attr native 的 z42.json 自有 extern（gen0-safe） |
| `src/libraries/z42.json/src/JsonPropertyAttribute.z42` | NEW | `[JsonProperty("name")]` |
| `src/libraries/z42.json/src/JsonIgnoreAttribute.z42` | NEW | `[JsonIgnore]` |

### tests + examples

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/libraries/z42.json/tests/serialize.z42` | NEW | [Test] 序列化 roundtrip |
| `src/libraries/z42.json/tests/deserialize.z42` | NEW | [Test] 反序列化到 typed 对象 |
| `src/libraries/z42.core/tests/reflection.z42` | MODIFY | Array 反射 + PropertyInfo attr 覆盖 |
| `examples/json_serde.z42` | NEW | 特性示例 |
| `src/tests/zbc-format/*/source.zbc` | MODIFY | 6 个 committed 字节基线 regen |
| `src/tests/zpkg-format/*/source.zpkg` | MODIFY | 4 个 committed 字节基线 regen |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | `empty/source.zbc` hex 基线重截 |

### docs

| 文件 | 类型 | 说明 |
|------|------|------|
| `src/libraries/z42.json/README.md` | MODIFY | 功能索引 + 核心文件 |
| `src/libraries/z42.core/src/README.md` | MODIFY | Array / PropertyInfo 反射面 |
| `docs/design/stdlib/json.md` | MODIFY | serde 机制节 |
| `docs/roadmap.md` | MODIFY | L2 JsonSerializer + Deserialize<T> 进度 + Deferred index |
| `docs/book/src/stdlib/json-serde.md` | NEW | serde 机制页（知识上浮，挂 SUMMARY） |

> **无格式 bump** → `docs/design/runtime/zbc.md` / `zpkg.md` / `.claude/rules/version-bumping.md` **不改**。

## 只读引用（理解上下文，不修改）

- `src/runtime/src/corelib/reflection.rs`（field attr / activator / ctor invoke 现有实现）
- `src/libraries/z42.core/src/Reflection/FieldInfo.z42`（attr API 镜像模板）
- `src/libraries/z42.core/src/Type.z42`、`src/libraries/z42.json/src/JsonValue.z42`
- `src/compiler/z42c.semantics/src/AttributeSynth.z42`（store-meta 工厂合成——本 change 补 PropertyDecl 分支）

## Out of Scope

- List<T> / Dictionary<K,V> / Set 等泛型容器（需泛型容器反射）→ Deferred。
- enum / nullable(`T?`) / char 的 serde → Deferred。
- camelCase ↔ PascalCase 命名策略 → Deferred。
- 循环引用检测 / `$ref` → 不做。

## Open Questions

- [x] serde 机制文档落 design/json.md 追加节 + 新建 book/json-serde.md（属性反射机制并入 book
      reflection 页）。—— 采纳：json.md 追加 serde 概览，book 落完整机制。
