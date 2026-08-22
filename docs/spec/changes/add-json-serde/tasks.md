# Tasks: JSON serde（对象 ↔ JSON）

> 状态：✅ 实施 + 本地 GREEN（**无格式 bump** → 单代自举本地可完整验）| 创建：2026-08-22 | 定案：2026-08-23

## 实施期关键调整
- **从 format-bump 回退到无格式 bump**（design Decision 2）：最初走「新增 `property_attributes` 段」，
  格式-bump 的 CI 两代自举链反复出现无法本地隔离的 gen1-stdlib 编译失败，投入巨大未定位根因 → 回退到
  「属性 attr 挂 `__prop_X` 背后字段、复用既有 field_attributes」。代价 = 计算属性不承载 attr（M2 限制）。
- **z42.core 改动回退**（Array.z42 / PropertyInfo.z42 不动）：冷启动 gen0 用种子 stdlib 编 z42.json →
  不能引用新 z42.core API（axis②）。反射底座改 z42.json-local extern（`JsonReflect.z42`）。见 design Decision 6.5。
- **跨包 imported-type footgun**：z42.json 读反射成员返回 Type/Attribute 须显式 cast（同 design 6.5）。
- **serde RUN 后暴露 3 个反射 gap 并修**（design Decision 6）：① AttributeSynth 漏 PropertyDecl 工厂
  ② FromJson 基元名 vs 装箱名 ③ 跨包泛型 typeof(T) 短名丢 handle（make_type_from_name 兜底）。

## 进度概览
- [x] 阶段 1: ~~格式底座~~ **不适用（无格式 bump）**
- [x] 阶段 2: runtime native（array 反射 + property attr 反射 + typeof 兜底）
- [x] 阶段 3: 编译器 emit（ClassDescBuilder 挂背后字段 + AttributeSynth PropertyDecl 工厂）
- [x] 阶段 4: ~~z42.core 反射面~~ **回退（改 z42.json-local extern）**
- [x] 阶段 5: z42.json serde 引擎
- [x] 阶段 6: 测试（stdlib z42.json 全绿）
- [ ] 阶段 7: 文档同步 + 归档 + PR

## 阶段 1: 格式底座 —— **不适用**
无格式 bump：`ZbcFormat`/`ZbcWriter`/`ZbcReader`/`ZpkgWriter`/`IrModule`/`bytecode.rs`/`loader.rs`/
`types.rs`/`zbc_reader.rs` 均不改（format-bump 尝试已回退）。

## 阶段 2: runtime native
- [x] 2.4 `corelib/reflection.rs`：`__property_custom_attributes` builtin（剥 get_/set_ → `__prop_<Name>`
      查 field_attributes）+ `make_type_from_name` 无点短名唯一简单名兜底（跨包泛型 typeof handle）
- [x] 2.5 `corelib/array.rs`：`__array_create` / `__array_get` / `__array_set` / `__array_length`
- [x] 2.6 `corelib/mod.rs`：注册 5 个 native

## 阶段 3: 编译器 emit
- [x] 3.1 `ClassDescBuilder.z42`：合成 `__prop_X` 背后字段时把属性 attr 填入其 `.Attrs`（既有 field_attributes 格式）
- [x] 3.2 `AttributeSynth.z42`：`_processMembers` 补 `PropertyDecl` 分支（属性 store-meta attr 合成工厂 + 记 FactoryFunc）

## 阶段 4: z42.core 反射面 —— **回退**
反射底座（array native + property-attr native）改为 z42.json-local extern（`JsonReflect.z42`），
z42.core 的 `Array.z42`/`PropertyInfo.z42` **不改**（axis② 冷启动约束，design 6.5）。

## 阶段 5: z42.json serde 引擎
- [x] 5.1 `JsonPropertyAttribute.z42` + `JsonIgnoreAttribute.z42`（`: Attribute`，D8 后缀）
- [x] 5.2 `JsonMember.z42`：字段+属性统一 + 键名/`[JsonIgnore]` 解析（`JsonMembers.For(Type)`）
- [x] 5.3 `JsonSerializer.z42`：公开 API + `_toJson`（序列化）
- [x] 5.4 `JsonBinder.z42`：`FromJson` + 构造绑定 + 数值 coercion（基元名 vs 装箱名两套词汇）
- [x] 5.5 `JsonReflect.z42`：array + property-attr native 的 z42.json-local extern

## 阶段 6: 测试
- [x] 6.1 `z42.json/tests/serialize.z42`（[Test]，全 scenario，8/8 绿）
- [x] 6.2 `z42.json/tests/deserialize.z42`（[Test]，全 scenario，10/10 绿）
- [ ] 6.3 `examples/json_serde.z42`（端到端演示）
- **无 fixture regen**（无格式 bump）。

## 阶段 7: 验证 + 文档 + 归档
- [x] 7.1 `cargo test --lib`（985+21 绿）
- [ ] 7.2 `xtask test`（完整 GREEN：stdlib 全绿 + 自举 gen1==gen2 不动点 + e2e）
- [ ] 7.4 spec scenarios 逐条覆盖确认
- [x] 7.5 文档：roadmap、json.md、book/json-serde.md（挂 SUMMARY）、本 change 4 文档（无格式 bump 化）
- [ ] 7.6 归档 + PR（本地即可完整 GREEN，无两代自举墙）

## 备注
- **无格式 bump → 本地即可完整 GREEN**（单代自举），无 macOS 两代自举墙、无 fixture 重生。
