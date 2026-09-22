# tasks：member-completeness-gaps（三合一）

状态：🟢 已完成并归档（2026-09-22）

## #1 跨包接口属性/索引器访问器
- 🟢 `ClassDescBuilder._interfaceDesc`：接口方法块补 PropertyDecl→get_X/set_X、IndexerDecl→get_Item/set_Item
- 🟢 新 `_IfaceMethodBuf` holder（收敛数组增长）
- 🟢 cross-zpkg fixture `iface_property_cross_pkg`（经接口静态类型跨包读属性 + 索引器，期望 25/square/8）

## #2 接口非法成员诊断 E0478
- 🟢 `MemberCollector._fillInterface`：FieldDecl / ClassDecl → E0478
- 🟢 单元门 ×4（static 字段 / 实例字段 / 嵌套类型 → E0478；合法成员 → ""）

## #3 比较/相等/位运算符重载
- 🟢 `MemberParser._operatorMethodName`（声明侧）+ `TypeFactsTc._operatorMethodNameTc`（派发侧）补 `== != < > <= >= & | ^ << >>`
- 🟢 e2e fixture `src/tests/operators/comparison_operator_overload.z42`（Ver ==/</>等 + Flags 位运算，interp+jit）

## 验证
- 🟢 三探针：#1 cross-zpkg fixture PASS（65/65 无回归）；#2 接口字段→E0478；#3 用户运算符端到端
- 🟢 #3 record == 爆炸半径：新旧 z42c 输出一致（零行为变化）
- 🟢 完整 GREEN（interp 全 stage ✅、24 单测、不动点 3/3、lines/walkers）
- 🟢 退回对照（三门各自变红，决定性）：
  - #1 stash ClassDescBuilder 重建 → `iface_property_cross_pkg` FAIL (main build)、其余 64 过
  - #2 基线 nightly 编接口 static 字段 → 静默通过（0 E0478）
  - #3 基线 nightly 编运算符 fixture → 14× E0402（`<` 不派发到用户 op_LessThan）
- 🟢 爆炸半径：完整 GREEN 全程新 E0478/E0404 命中 0
- 🟢 jit 补充：e2e jit ✅ / cross-zpkg jit ✅ / bootstrap NO violation ✅ / stdlib jit 340 files ✅（清建 0 E0478/E0404）

## 文档
- 🟢 `error-codes.md`：E0478 条目 + 码族范围到 E0478
- 🟢 `operators.md`：新增「运算符重载（用户类型）」节（op_ 名表）
- 🟢 `interfaces.md`：「不能出现在接口里的成员」（E0478）+ 跨包访问器保真注释
- 🟢 `generic-constraints.md`：订正 #727 的「需格式 bump」残余边界（其实无需 bump）

## 归档
- 🟢 `changes/` → `archive/2026-09-22-member-completeness-gaps/` + tasks 🟢 + PR（同一 PR 内）
