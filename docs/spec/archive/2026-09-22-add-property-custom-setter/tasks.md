# tasks：add-property-custom-setter

状态：🟢 已完成并归档（2026-09-22）

## User 裁决（2026-09-22）
- 🟢 完整支持自定义 setter（非只做干净报错）
- 🟢 混合访问器（一半 auto 一半带体）报错 E0474

## 代码（7 处）
- 🟢 `PropertyDecl`（Decl.z42）加 `HasSetBody`/`SetBody` + ctor 初始化 + Dump
- 🟢 `MemberParser._parseProperty`：带体分支泛化到 get+set（`set { }` / `set => e;`）
- 🟢 混合访问器诊断 **E0474**（parser 字面量发码）
- 🟢 `MemberCollector`：`pfHasStorage` 加 `&& !HasSetBody`
- 🟢 `DeclBinder`：`HasSetBody` → 绑 set_X 体（value 参 + this，镜像索引器 set_Item）
- 🟢 `IrGenMemberEmitter.EmitProperty`：`HasSetBody` → 发真实 set_X（链外单发）
- 🟢 `ClassDescBuilder`：后备字段条件加 `&& !HasSetBody`

## 测试
- 🟢 单元门 ×6（`property_access_tests.z42`）：带体 get+set→"" / 表达式体 setter→"" / 混合两向→E0474 ×2 / 两 auto→"" / value 在作用域→""
- 🟢 运行期 e2e `src/tests/classes/custom_property_setter.z42`（副作用 setter、变换 setter、表达式体 setter、经接口写）interp+jit

## 验证
- 🟢 退回对照（e2e 决定性）：基线 nightly 编同 fixture → **69 errors**（parser 级联 E0202/E0443）；我的 z42c → 编译+运行 clean
- 🟢 E0474 探针：`{ get; set {} }` / `{ get {} set; }` 各精确 1 条 E0474
- 🟢 自举不动点 3/3 gen1==gen2（改了 z42c.syntax 仍零漂移）
- 🟢 完整 GREEN：interp 全 stage ✅ / e2e jit ✅ / cross-zpkg jit ✅ / stdlib jit 339 files ✅ / bootstrap NO violation ✅
- 🟢 爆炸半径：`rm .cache` 全量 build stdlib 25/25 → 新 E0474 命中 0（生产代码无混合访问器）

## 文档
- 🟢 `properties-indexers.md`：「计算属性 getter」节改「带体访问器 get/set」，补自定义 setter + E0474 混合禁令；摘要表补带体形态
- 🟢 `error-codes.md`：E0474 条目 + 字面量码族范围更到 E0474

## 归档
- 🟢 `changes/` → `archive/2026-09-22-add-property-custom-setter/` + tasks 🟢 + PR（同一 PR 内）
