# tasks：fix-iface-property-setter

状态：🟢 已完成并归档（2026-09-22）

## 代码
- 🟢 `MemberCollector._fillInterface`：`PropertyDecl` 分支补 `set_X(value)` 方法符号（`pd.HasSet` 时）
- 🟢 `AssignTyper` 属性 setter 路径：新增 `Z42InterfaceType` 收者分支（解析 `set_X` + box/convert value + 发 instance 虚调用）

## 满足性校验（零改动，自动生效）
- 🟢 `set_X` 进 `it.Methods` ⇒ `_checkOneIfaceMethod` 自动要求实现方补齐（MangleKey 含名，get_X/set_X 独立契约）

## 测试
- 🟢 单元门 ×4（`property_access_tests.z42`）：
  - `test_iface_property_getset_impl_complete_ok`（get+set 齐 → ""）
  - `test_iface_property_missing_setter_e0412`（只 getter → E0412，**新增强制**）
  - `test_iface_property_getonly_no_setter_required_ok`（getter-only 接口不要求 setter → ""，不回归）
  - `test_iface_property_getset_explicit_methods_ok`（显式 get_N/set_N 方法满足契约 → ""）
- 🟢 运行期 e2e `src/tests/interfaces/interface_property_setter.z42`（经接口读+写属性，含值类型 box/convert）

## 验证
- 🟢 退回对照（单测）：stash MemberCollector 改动重建 → `test_iface_property_missing_setter_e0412` **精确变红**（FirstErrorCode ""），其余 3 正例保持 PASS
- 🟢 退回对照（e2e）：基线 nightly 编同 fixture → **运行期 FAIL**（`values not equal`，`m.Name = "renamed"` 静默丢弃），我的 z42c → exit 0
- 🟢 自举不动点 3/3 gen1==gen2（无字节漂移）
- 🟢 完整 GREEN（`xtask test` interp 全 stage ✅ / e2e jit ✅ / cross-zpkg jit ✅ / stdlib jit 339 files ✅ / bootstrap NO violation ✅）
- 🟢 爆炸半径量测：`rm .cache` 全量 build stdlib + z42c 自建 → 新 E0412 命中 **0**（生产接口零属性 setter，闭洞型）

## 文档
- 🟢 `interfaces.md`：「方法与属性」节补 `{ get; set; }` 接口属性（setter 独立契约 + 经接口写）
- 🟢 `generic-constraints.md`：订正两处过期表述（「接口方法齐备性今天仍不校验」是 #636/#657 后的谎言）

## 归档
- 🟢 `changes/` → `archive/2026-09-22-fix-iface-property-setter/` + tasks 🟢 + PR（同一 PR 内）
