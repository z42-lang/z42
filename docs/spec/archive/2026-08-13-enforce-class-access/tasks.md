# Tasks: 类级访问强制（同包 private/protected 嵌套类）

> 状态：🟢 已完成（①同包，本地 GREEN）| 创建：2026-08-13 | 分支：`enforce-class-access` | worktree：`../z42-classacl`
> 拆分：②跨包 internal 类（格式 bump）→ follow-up `enforce-crosspkg-internal-class`（走 CI，见 memory 口令）。

## 进度概览
- [x] 阶段 1: 类可见性上 `Z42ClassType`（内存态，无格式）
- [x] 阶段 2: 引用点强制 `CheckTypeRef`（体引用 + 声明签名，覆盖同包嵌套 private/protected）
- [x] 阶段 3: 测试（体引用单元 + 声明签名单元）
- [x] 阶段 4: 完整 GREEN（warm 0.37）
- [x] 阶段 5: 文档同步 + 归档

## 阶段 1: 类可见性（本地内存态）
- [x] 1.1 `Z42Type.z42`：`Z42ClassType` 加 `string Visibility`（默认 "public"）
- [x] 1.2 `IrGenFacts`：`classVisCode/classVisStr/classVis`（位置默认单一真相：`+`名→private / 顶层→internal / 显式优先）
- [x] 1.3 `SymbolCollector._putClassStub`：`ct.Visibility = IrGenFacts.classVis(c.Mods, c.Name.IndexOf("+")>=0)`

## 阶段 2: 引用点强制
- [x] 2.1 `AccessChecker` 静态 `CheckTypeRef(...)` + `_nestedOuter` + private/protected 判定（`_derivesFromOrEq`）+ internal 分支（本地放行 / imported 因默认 public 不触发）
- [x] 2.2 `TypeChecker._chkTypeRef(t, env, sp)` 包装（emit `_diags`）
- [x] 2.3 `ExprTyper`：new / cast / is / as / typeof / default(T) 解析后调 `_tc._chkTypeRef`
- [x] 2.4 `StmtBinder`：局部 var / catch 解析后调 `_tc._chkTypeRef`
- [x] 2.5 `SymbolCollector._chkTypeRef` 助手 + `_fillClass`（字段/属性/索引器/基类·接口）+ `_methodSymbol`（参/返回）解析后调（emit `this.Diags`）
- [x] 2.6 `DiagnosticCodes.z42`：E0404 注释补类型引用
- [x] 2.7 局部验证：临时 fixture（private 嵌套越界，体 + 声明各一）确认 E0404；z42c 自建仍绿（自身无越界）

## 阶段 3: 测试
- [x] 3.1 `access_control_tests.z42`：体引用类级单元（private/protected 嵌套 new/var/is 越界→E0404、外层类内 OK、public OK、派生类 OK、顶层 internal 同包 OK）
- [x] 3.2 `collect_tests.z42`：声明签名位置类级单元（字段/参/返/基类为 private 嵌套→E0404、public/同包 internal OK、外层类自有字段 OK）

## 阶段 4: 验证
- [x] 4.1 `cargo build --release`（z42vm）通过
- [x] 4.2 `xtask test`（warm 0.37 完整 GREEN gate：e2e / cross-zpkg / stdlib / compiler / vscode-syntax）
- [x] 4.3 z42c 自举 `gen1==gen2` 逐字节保持（纯诊断，`test_zbc_empty_byte_identical` 绿）
- [x] 4.4 spec scenarios（体引用 + 声明签名）逐条覆盖

## 阶段 5: 文档 + 归档
- [x] 5.1 `docs/design/language/access-control.md`：Status → 类级同包强制已实现 + 跨包 internal 列 Deferred
- [x] 5.2 `docs/book/src/compiler/access-control.md`：加「类级访问强制」节（引用点 / 嵌套 outer / 两相位；跨包 internal Deferred）
- [x] 5.3 doc-check + 归档 mv → `docs/spec/archive/2026-08-13-enforce-class-access/`
- [x] 5.4 commit + PR

## 备注
- **拆分缘由**：②跨包 internal 需类可见性进 zbc/zpkg（格式 bump），本地格式-bump 自举撞 **macOS 两代自举墙**
  （`escape-stack-format-bump-ci-learnings`：gen0 stdlib 本地产坏、gen1 z42c 解析不到 z42.ir 类型，CI/Linux 正常）。
  User 裁决拆分：① 本地 GREEN 先落地，② 走 CI。② 完整代码已实现并存 patch（见 memory `add-crosspkg-internal-class`）。
- **破坏面≈0**：z42c 235/235 public、stdlib 生产 337/337 public（34 无修饰符全为测试 fixture 自引用、无嵌套越界）。
- worktree 供种：0.37 SDK nightly（2026-08-12）作 `.z42` 种子；`cp -R` 主树 artifacts 再 `rm compiler/libraries` warm 重建。**别设 `Z42_HOME`**。
