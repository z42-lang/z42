# Tasks: 强制访问权限检查

> 状态：🟢 已完成 | 创建：2026-08-12 | 完成：2026-08-12

## 进度概览
- [x] 阶段 1: IsImported 地基
- [x] 阶段 2: AccessChecker + 接入 + C# 一致性规则（override / record）
- [x] 阶段 3: 测试与验证（全 GREEN + 自举不动点 + cargo + REPL）
- [x] 阶段 4: 文档同步

## 阶段 1: 基础
- [x] 1.1 `Z42Type.z42`：`Z42ClassType` 增 `bool IsImported`（默认 false）
- [x] 1.2 `ImportedSymbolLoader.z42`：imported 类型置 `IsImported = true`

## 阶段 2: 核心实现
- [x] 2.1 `AccessChecker.z42`（NEW）：`CheckAccess` + protected 基链上溯
- [x] 2.2 `TypeChecker.z42`：`_access = new AccessChecker(this)`
- [x] 2.3 `MemberResolver._bindClassMemberAccess`：字段 / getter / 方法组接入
- [x] 2.4 `MemberResolver._bindInstanceMemberCall`：实例方法调用接入
- [x] 2.5 `MemberResolver._bindMember`：静态字段读接入
- [x] 2.6 `MemberResolver._bindMemberCall`：静态方法调用接入
- [x] 2.7 `ExprTyper._bindAssign`：属性 setter 接入（字段写经 2.3 覆盖）
- [x] 2.8 跨包 internal：`_visCode` 无修饰符→3 + `_visStr` 3→"internal"（**无格式 bump**，u8 扩值域）
- [x] 2.9 C# 一致性规则：override 继承基类可见性（`_vis`/`_visCode` override→public）
- [x] 2.10 C# 一致性规则：record 定位字段 public（`DeclParser`）
- [x] 2.11 编译器 split 辅助类 45 处 `private→internal`（同包协作欠标注修正）

## 阶段 3: 测试与验证
- [x] 3.1 `z42c.semantics/tests/access-control/`：14 个单元（private/protected/internal-同包/public/静态/违规 E0404）
- [x] 3.2 golden 修正：record-public AST ×3、vis=3 hex ×2
- [x] 3.3 `cargo test`（z42vm + 集成）：889 passed, 0 failed（反射 vis=3 无回归）
- [x] 3.4 `xtask test compiler`：23 units + **自举 5/5 gen1==gen2 字节不动点**
- [x] 3.5 `xtask test`：**✅ GREEN 全 stage**（e2e / cross-zpkg / stdlib / compiler / vscode）
- [x] 3.6 跨包 internal（字段+方法）+ override + record + public：手工端到端逐案验证 E0404/放行
      —— cross-zpkg run-and-compare harness 无 expected-compile-error 模式，跨包 internal 的**逻辑**由
      3.1 单元（CheckAccess + IsImported）+ 本手工验证覆盖；stdlib 构建本身即海量跨包 public 回归门
- [x] 3.7 REPL 手验（新 z42.scripting）：`class A{private int a;} A a=new(); a.a` → `E0404: cannot access private field 'a' of 'A'` ✅

## 阶段 4: 文档同步
- [x] 4.1 `docs/book/src/compiler/access-control.md`（NEW）+ 挂 SUMMARY.md
- [x] 4.2 `z42c.semantics/README.md`：核心文件补 AccessChecker
- [x] 4.3 proposal/design/spec 同步实现现实（无 bump、override/record 规则、45 re-marks）

## 备注
- **无格式 bump**（实证纠正初判）：成员可见性 `u8` 加值 3=internal 不改布局；reader 原样携带、反射比较仍正确。
- 反射副作用（正确化）：无修饰符成员现 `IsPublic=false`（C# 语义下 internal 非 public）。
- Deferred：跨包多层继承成员 internal 保真长尾（design Deferred）；as/is + 反射运行时绕过另立 change。
