# Tasks: add-implicit-base-ctor-call

> 状态：🟢 已完成（v2 User 2026-09-15 确认；Exception() 不补）
>
> 2026-09-15 恢复：前置 #660（构造器可见性强制）已合并，rebase 到 `1445565de`。暂停期唯一失败的单测改为基类构造器写 `public`。

- [x] 1.1 修前回归固化：`classes/implicit_base_ctor.z42`、`classes/inherited_ctors.z42`、跨文件、cross-zpkg 夹具，逐条确认红
- [x] 2.0 `ConstructTyper._bindCtorArgs` 抽出：`new` 与初始化子句共用构造器选择 + 实参适配
- [x] 2.1 合成构造器标记位（`MethodDecl`，构造后置位）；`_hasExplicitInstanceCtor` 排除合成
- [x] 2.2 收集期：基类先于派生的拓扑序；无显式 ctor 的类注册继承构造器（D3）或默认构造器
- [x] 2.3 `DeclBinder`：隐式 `base()` 注入 + 新诊断（D6）；继承 / 默认构造器体；删祖先链内联（D5）
- [x] 2.4 `IrGenTypeEmitter` 发射对齐；确认继承构造器进 TSIG（含默认值 / params / caller 元数据）
- [x] 3.1 单测（诊断 / 继承签名 / 泛型代换 / E0426 与继承集）
- [x] 3.2 两处测试补 `: base(..)`（`basic/inherited_fields`、`inheritance/inheritance`）
- [x] 4.1 文档：book `constructors.md` 改写（隐式 base + 继承规则 + 语言对比），删已知缺口；新机制页 `compiler/ctor-inheritance.md`；language-overview §6.3 旧模型两条划掉指向 book；`DiagnosticCodes.BaseCtorRequired = E0469`；cross-zpkg README
- [x] 4.2 **（恢复后）** #660 的可见性检查落在共享 `_bindCtorArgs` 内 ⇒ 同一处同时覆盖 `new` 与初始化子句；④ 单测源码串基类构造器补 `public`
- [x] 4.3 **（恢复后）** 基类构造器全是 private 时：继承不到 ⇒ 合成默认构造器走隐式 `base()`，报 E0404 / E0469（不静默默认构造）
- [x] 4.4 **（#661 规则）** `CacheStore.CompilerFingerprint` 5 → 6（codegen / TSIG 输出变化；兼覆盖 #660 未累加的主构造器可见性字节）
- [x] 5.1 两后端手验（interp / jit：两个运行期用例 + cross-zpkg）；全量 GREEN（`GREEN_EXIT=0`，基于 main `1445565de`）；`xtask test bootstrap` ✅；本地不编译目录逐文件扫描无新诊断
