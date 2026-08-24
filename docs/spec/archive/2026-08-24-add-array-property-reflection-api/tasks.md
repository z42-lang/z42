# Tasks: 反射式数组 + 属性 attribute 反射 API 下沉

> 状态：🟢 已完成 | 创建：2026-08-23 | 完成：2026-08-24

## 进度概览
- [x] 阶段 1: Phase 1 公开 API（z42.core）+ runtime 小改
- [x] 阶段 2: Phase 1 测试
- [x] 阶段 3: Phase 2 改用公开 API + 删 extern（z42.json）
- [x] 阶段 4: 验证（GREEN）+ 文档同步

## 阶段 1: Phase 1 公开 API（z42.core）+ runtime 小改
- [x] 1.1 `Array.z42` 加 C# 风格 API：静态 `CreateInstance(Type,int)->Array` + 实例 `GetValue(int)` + 实例 `SetValue(object value,int index)`（长度用既有 `Length`），绑 `__array_create`/`__array_get`/`__array_set`
- [x] 1.2 `corelib/array.rs`：`builtin_array_set` 重排读 `(array,value,index)`；删 `builtin_array_length` fn；`corelib/mod.rs` 删 `__array_length` 注册
- [x] 1.3 `cargo build --release` + `cp` 同步 `.z42/bin/z42vm`（改 runtime 后必做）
- [x] 1.4 `Reflection/PropertyInfo.z42` 加 `__attrCache` 字段 + `__customAttributes` extern + `GetCustomAttributes()` + `GetAttribute(Type)`（镜像 FieldInfo，用 `__getterQualified ?? __setterQualified`）

## 阶段 2: Phase 1 测试
- [x] 2.1 NEW `z42.core/tests/reflection_api_downsink.z42`：反射数组 roundtrip（int/string/编译期数组）
- [x] 2.2 同文件：PropertyInfo attr（命中 / null / 计算属性空 / 多 attr 全返回）
- [x] 2.3 `xtask test stdlib z42.core` 通过（新 [Test] 全绿）

## 阶段 3: Phase 2 改用公开 API + 删 extern（z42.json）
- [x] 3.1 `JsonBinder.z42`：`Array arr = Array.CreateInstance(elem,n)`、`arr.SetValue(v,i)`（确认 `using Std;`）
- [x] 3.2 `JsonSerializer.z42`：hoist `Array a=(Array)o;` → `a.Length` / `a.GetValue(i)`
- [x] 3.3 `JsonMember.z42`：`JsonReflect.PropAttr(p,typeof(X))`→`p.GetAttribute(typeof(X))`
- [x] 3.4 `JsonReflect.z42`：删 5 个 extern（`__array_*`×4 + `__property_custom_attributes`）+ `PropAttr` 辅助（保留集合反射辅助）
- [x] 3.5 `xtask test stdlib z42.json` 通过（serde 行为不变）

## 阶段 4: 验证（GREEN）+ 文档同步
- [x] 4.1 `cargo build --release`（z42vm）无错（含 array.rs 重排/删除）
- [x] 4.2 `xtask test` 完整 GREEN（e2e / cross-zpkg / stdlib / compiler 自举 5/5 / vscode-syntax）
- [x] 4.3 spec 场景逐条覆盖确认
- [x] 4.4 `z42.core/src/README.md` 功能索引加 Array 反射 + PropertyInfo attr 行
- [x] 4.5 `docs/book/src/stdlib/json-serde.md`「反射底座的自举约束」节改写为「已下沉公开 API」
- [x] 4.6 `docs/design/language/reflection.md` 公开 API 列表补 Array 反射静态 + PropertyInfo attr（按 6.5 裁决）
- [x] 4.7 `docs/roadmap.md` Deferred `json-serde-future-public-reflection-api` 标记完成

## 备注
- **零格式 bump**：runtime 仅 2 处受控小改（array.rs 重排 + 删死 native），无 zbc/zpkg 格式变化 → self-host 字节不动点仍成立。
- **两阶段同 PR 安全性**：已核实（proposal 顶部 + design）——workspace 自洽解析 fresh z42.core，本地 warm 可验。
- **C# 设计（User 定）**：照搬 `System.Array`——静态 `CreateInstance` + 实例 `GetValue`/`SetValue(value,index)` + `.Length`；`(Array)o` 下转型已核实成立（design Decision 1/4）。
- 6.5 待裁决：Open Question「reflection 文档 book 迁移是否本 change 顺带做」（design Decision 3 推荐：不做，仅更新 reflection.md 列表）。
