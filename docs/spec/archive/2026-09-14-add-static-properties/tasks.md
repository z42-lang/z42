# Tasks: add-static-properties

> 状态：🟢 已完成 | 创建：2026-09-14 | 完成：2026-09-14（DRAFT 2026-09-14 User 确认：4 项一起做） | 类型：lang（语义层；无语法/IR/格式改动）

## 阶段 0：事实基线
- [x] nightly `2843abebe` 编译器实测静态属性全形态、裸名静态字段/const、`C.f += v`、属性初始化器（见 proposal 表格）
- [x] census pass：把 D4 判据搬成只打 W9999 的探针，stdlib / z42c / xtask 全量 `--no-incremental` 重建 ⇒ **零命中**
  （正向对照：往 z42.encoding 塞一个命中文件，workspace 模式下确实报出）⇒ 预期零代码生成漂移

## 阶段 1：静态属性
- [x] 1.1 `BoundStaticGet` 加 `IsProp/PropSetter/PropBacking`
- [x] 1.2 `MemberCollector` 拆 `pfHasStorage`/`pfHasBacking`：静态 auto 填 `PropBackingName`、不进实例布局
- [x] 1.3 `StubEmitter` 静态桩发 `static_get/static_set`（FQ 后备名；静态桩 debug locals 不再登记 this）
- [x] 1.4 `MemberResolver.BindStaticMember`（新文件 `MemberResolver.Static.z42`，partial——主文件会超 886 行硬限）
- [x] 1.5 发射分叉：`ExprEmitter` 读 / `AccessEmitter._emitStaticStore` 写 / `OperatorEmitter` 写回；
      访问器调用 `CallEmitter._emitStaticAccessor`（抽出 `_depStaticTarget` 与 `_emitCall` 共用）
- [x] 1.6 `AssignTyper` E0452（节点元数据判定；静态 ctor 放行本类 get-only auto）
- [x] 1.7 静态计算 getter 体绑定为静态上下文

## 阶段 2：裸名静态成员 + `C.x += v`
- [x] 2.1 `_bindIdent` 接 `BindBareStatic`
- [x] 2.2 `DeclBinder._defineInstanceFields`（6 处手写循环收敛）只 Define 非 static；`FunctionEmitter.ctx.Fields` 只收非 static
- [x] 2.3 `AssignTyper._isStaticRecv`：`+=`/`-=` 分支与 `=` 分支共用

## 阶段 3：属性初始化器
- [x] 3.1 4 个注入点加属性分支（合成 ctor / `_injectFieldInits` / per-CU `__static_init__` / `_injectStaticFieldInits`）
- [x] 3.2 `AssignTyper.BindInitValue` + `_finishValue`（赋值尾段抽出，字段/属性共用）
- [x] 3.3 计算属性带初始化器 → E0452（`_bindClass` 每声明报一次）

## 阶段 4：跨包
- [x] 4.1 cross-zpkg 正例 + 负例
- [x] 4.2 `ClassExtractor` 导出静态访问器——**退回对照校正了设计**：跨包符号实际由 `TsigReconcile` 从 IR 签名重建、
      本就含静态访问器；关掉导出正例照过。保留导出只为进程内导出模型与重建结果一致（design D7 已改写）

## 阶段 5：测试
- [x] `src/tests/classes/static_properties.z42`
- [x] `src/tests/classes/static_members_bare_name.z42`
- [x] `src/tests/classes/property_initializers.z42`
- [x] `z42c.semantics/tests/typecheck/static_property_tests.z42`（E0452 ×6、静态上下文 ×2、裸名、`+=`、TSIG 导出）
- [x] `src/tests/cross-zpkg/static_property_cross_pkg` + `static_property_crosspkg_readonly`
- [x] interp + JIT 双后端（3 个 classes 用例手工两后端全过）
- [x] 修前红：3 个 classes 用例在 nightly 编译器下全部编不过（E0401 ×19）

## 阶段 6：文档 + GREEN + 归档
- [x] `member-accessors.md`：静态属性节（含 mermaid 实现原理）+ 初始化器语义
- [x] 新页 `static-members.md`：裸名解析顺序 + 不支持项 + 历史 + 实现；SUMMARY 挂载
- [x] `static-constructors.md`：登记跨包 cctor 已知缺陷
- [x] 新页「不支持」三条（派生类裸名基类静态 / 嵌套类裸名外层静态 / 静态字段初始化器裸名）逐条实测为 E0401；实例字段初始化器裸名静态可用（两后端 3）
- [x] `DiagnosticCodes.z42` E0452 注释补静态/初始化器形态
- [x] GREEN（`xtask test` 全 13 stage，GREEN_EXIT=0，含新单测 13 条 + 3 个 classes e2e + 2 个 cross-zpkg）+ `xtask test bootstrap`（nightly z42c 编当前源 ✅）；rebase 后重跑见 PR body
- [x] 零代码生成漂移：修前 / 修后编译器同模式重建 z42.core/build/ir/project/collections/text、z42c.core/syntax、xtask **逐字节相同**
- [x] 归档随 PR

## 发现的既有 bug（均在 nightly `2843abebe` 复现，与本 change 无关，**未修**，另行登记）

1. **静态 ctor + 无参实例 ctor 同类 ⇒ 实例 ctor 永不执行**：`obj_new` 指向 `C.C$0`，函数却发成 `C.C`（两者按名同 arity 被
   当重载 mangle 了调用侧、没 mangle 定义侧）。静默：对象字段停在默认值。
2. **`: this()` 零实参委托 ⇒ 目标 ctor 不被调用**（`: this(x, 1)` 正常）。静默。
3. **跨包静态方法调用不触发依赖包类型的静态 ctor**：惰性加载只在 `try_lookup_type` 登记 cctor，函数查找不走那里
   ⇒ `any_cctor_pending()` 恒假、屏障跳过。静默：静态字段停在默认值。（已写进 `static-constructors.md` 已知缺陷）
