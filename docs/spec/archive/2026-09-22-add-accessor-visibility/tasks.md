# tasks：add-accessor-visibility

状态：🟢 已完成并归档（2026-09-22）

## 代码（4 处）
- 🟢 `MemberCollector._fillClass`：get_X/set_X 符号 vis 从 `pd.GetMods`/`pd.SetMods`（覆盖属性级）
- 🟢 `IrGenMemberEmitter.EmitProperty`：5 个 accessor 发射点设 `.Visibility`
- 🟢 `FieldSymbol` 加 `SetterVis` + `MemberCollector` 在 `pd.SetMods != ""` 时设
- 🟢 `AssignTyper` 属性写路径：`SetterVis` 非空时对 setter 做 `CheckAccess` → E0404

## 测试
- 🟢 单元门 ×5（`property_access_tests.z42`）：private set 外部写→E0404 / 类内写→"" / 外部读→"" / public set 外部写→"" / protected set 外部写→E0404
- 🟢 运行期 e2e `src/tests/classes/private_set_property.z42`（private-set 类内可写、外部只读、功能正常）interp+jit

## 验证
- 🟢 探针：外部写 private set → `E0404: cannot access private property setter`；类内写 + 外读 → clean
- 🟢 退回对照（决定性）：基线 nightly 编外部写 → clean（无 E0404）；我的 z42c → E0404
- 🟢 自举不动点 3/3 gen1==gen2
- 🟢 完整 GREEN（interp 全 stage）+ 全程 0 E0404（爆炸半径 0）
- 🟢 jit 补充：e2e jit ✅ / cross-zpkg jit ✅ / bootstrap NO violation ✅ / stdlib jit 340 files ✅
- 🟢 爆炸半径清建复核：`rm .cache` 全量 build stdlib 25/25 → 新 E0404 命中 0

## 文档
- 🟢 `properties-indexers.md`：「访问器级可见性修饰符：解析但当前忽略」→ 订正为「生效」（E0404 + 反射真值 + 实现说明）；Age 例注释更新

## 归档
- 🟢 `changes/` → `archive/2026-09-22-add-accessor-visibility/` + tasks 🟢 + PR
