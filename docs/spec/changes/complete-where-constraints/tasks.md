# tasks — complete-where-constraints

> 状态：🟡 IMPL 进行中（gate 已过 2026-09-05）| 创建：2026-09-05 | 类型：lang（完整流程）
> 图例：⚪ 未开始 · 🟡 进行中 · 🟢 完成 · ⛔ 阻塞
>
> **分支/worktree**：`complete-where-constraints` @ `../z42-whereconstraints`（基于 origin/main）
>
> **落地形态**：本 change 分 **4** 个 PR（PR-0 已撤销并进 PR-1）。PR-A 与 where 语义无关、
> 有独立价值，可先行合并；PR-1/2/4 是主体。PR-3 / PR-5 **不在本轮**（见 design §6）。

---

## Phase 0 — 测试机制（PR-0）🟢 已撤销

> **撤销理由（2026-09-05 IMPL 期实测）**：机制已存在且已在 GREEN gate 内——
> `SemanticDump.FirstErrorCode/FirstErrorMessage/ErrorCount` + `[Test]` units，由
> `xtask test compiler`（stage 5）驱动，活例 `undefined_type_tests.z42`。且初稿选的落点
> `xtask_test_dist.z42` **不在** `xtask test` 的 stage 表里（`test dist` 是发行版专用子命令）。
> 详见 design §4 PR-0 段。

- [x] 0.1–0.3 ~~sidecar 机制~~ **撤销**：不另造第二套，用既有 `SemanticDump` + `[Test]`
- [x] 0.4 `src/tests/README.md` 第 44 / 73 行指向已删的 `src/compiler/z42.Tests/Fixtures/`
      → 改为指向 `SemanticDump` 单测体系（由 1.11 一并落地）
- [x] 0.5 ~~自举一条现有 fixture~~ **撤销**：`undefined_type_tests.z42` 已是活例，无需再证
- [x] 0.6 跨包 / 多文件负例门（真正仍缺的那部分）登记 Deferred
      `where-constraint-future-crosspkg-negative-gate`（design §7）

## Phase A — 修 ZbcReader 漏读 bit2（PR-A，独立 bug fix）⚪

- [ ] A.1 `ZbcReader.z42` TYPE 约束段：补读 `bit2 base u32`、`bit6 funcSig`，
      与 `ZpkgReader._skipConstraintBundle` 的布局对齐
- [ ] A.2 加一条 zbc round-trip 回归：构造置了 bit2 的 bundle → 读回不错位
- **GREEN**：`xtask test` 全绿 + `zbc-format` / `zpkg-format` 用例不动
- **注**：这条是 PR-3 的硬前置；本轮即使不做 PR-3 也应先修（格式契约单侧退化）

## Phase 1 — 接口约束（PR-1，主体）⚪

### 1a 重构先行（单独 commit）
- [ ] 1.1 合并 `_fillBundle` / `_fillBundleM` 为一份（两者今天逐字重复）
- [ ] 1.2 合并 `_checkBundle` / `_checkBundleM`（差异仅 Span 与 owner 名 → 参数化）
- **GREEN**：纯重构，行为不变，字节不动点成立（gen1 == gen2）

### 1b 接口约束（功能 commit）
- [ ] 1.3 `ConstraintBundle` 加 `InterfaceNames[] / InterfaceCount`
- [ ] 1.4 **去掉 `nt.ArgCount == 0` 过滤**；按裸名分流：`HasInterface` → 接口约束，
      `HasClass` → base-class 约束
- [ ] 1.5 `_checkBundle` 加接口分支 → `SymbolTable.Implements`
- [ ] 1.6 **接口继承闭包**：`Z42InterfaceType` 加 base 列表 + `Implements` 走闭包
      （本 PR 主要未知量；代价超预期则退为「只判直接接口 + 不升 error」，见 design §4）

### 1c warning 探针 → 翻 error（User 裁决）
- [ ] 1.7 先以 **warning** 发诊断，`xtask test` + 全 stdlib 编译，**拉出新增诊断完整清单**
- [ ] 1.8 逐条对账；重点验 `int`/`string` → wrapper 归一（`Dictionary<int,int>` /
      `HashSet<string>` / `PriorityQueue<long>` / `SortedSet<int>` …）
- [ ] 1.9 `src/tests/types/struct_generic_container.z42`：给 `struct P` / `struct Tagged`
      补 `: IEquatable<>`（User 已裁决）
- [ ] 1.10 清单归零后**翻成 error**
- [ ] 1.11 负例单测：新建 `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42`
      （与 `typecheck_tests.z42` 平级、扁平、单一职责），用 `SemanticDump.FirstErrorCode`
      断言「接口约束不满足 → E04xx」+ 正例「满足 → 零诊断」；同时修
      `src/tests/README.md:44,73` 指向已删 `z42.Tests/Fixtures/` 的两条死链（承接 0.4）
- **GREEN**：`xtask test` 全绿 + `test stdlib --mode jit` + 字节不动点

## Phase 2 — `enum` + `new()`（PR-2）⚪

- [ ] 2.1 `TypeParser._parseConstraint` 补 `enum` special（今天落进 `_parseType()` 兜底）
- [ ] 2.2 `WhereConstraint.Special` 注释同步取值（`new()` / `class` / `struct` / `enum`）
- [ ] 2.3 `ConstraintBundle` 加 `RequiresEnum` / `RequiresCtor`
- [ ] 2.4 `enum` 判定 → `symbols.EnumTypes.ContainsKey`
- [ ] 2.5 `new()` 判定 → `ct.OverloadsOf(ct.Name())` 找 `ParamCount==0`；
      **无显式 ctor = 满足**、abstract 类**不**满足（对齐运行期 `generics.rs`）
- [ ] 2.6 负例用例各一条（Phase 0 机制）
- **GREEN**：`xtask test` 全绿。`new()` 全仓零真实用例 → 回归面为零；
      `enum` 仅 `src/tests/generics/generic_enum_constraint.z42` 一条

## Phase 4 — 诊断质量（PR-4）⚪

- [ ] 4.1 ConstraintChecker 全程改用真 Span（`WhereClause.Span` / `WhereConstraint.Span` 本来就有）
- [ ] 4.2 删 `_noSpan()`
- [ ] 4.3 未知约束名（拼写错误）从静默改报 **E0443 UndefinedType**
- [ ] 4.4 负例用例：`where T : IFooo` → E0443
- **GREEN**：`xtask test` 全绿；确认既有诊断的 Span 变化不打翻 golden

## Phase 9 — 文档 + 归档 ⚪

- [ ] 9.1 `docs/book/src/language/` 新建约束页（或并入 generics 页）：七项约束语义 +
      **裸名匹配的已知限制**（D1）+ 跨包尚不校验（诚实标注）
- [ ] 9.2 `docs/design/language/generics.md` 的「约束体系」表按实况更新
      （当前标 ✅ 但实际未校验的行必须改）
- [ ] 9.3 `GenericConstraint.z42` 那条**已过时**的「z42c 缺对应类型/信息」注释删除/改写
- [ ] 9.4 Deferred 六条登记进 roadmap 未完成项索引（见 design §7）
- [ ] 9.5 `src/tests/README.md` 已在 0.4 更新——复核
- [ ] 9.6 归档 `changes/` → `archive/2026-XX-XX-complete-where-constraints/`，**随 PR 一起提交**

---

## 风险登记

| 风险 | 等级 | 处置 |
|---|---|---|
| 基元 → wrapper 归一有偏差 → `Dictionary<int,int>` 编不过 → **自举链断** | 🔴 | Phase 1c warning 探针先量（User 裁决） |
| 接口继承闭包代价超预期 | 🟠 | 退为只判直接接口 + 不升 error（design §4） |
| 去 `ArgCount` 过滤让 21 条泛型接口约束一次性生效 | 🟠 | 同 warning 探针 |
| Span 变更打翻既有诊断 golden | 🟡 | Phase 4 独立 PR，golden 变化单独对账 |
| 跨包 100% 不校验（本轮不修）→ 用户以为已保护 | 🟡 | 文档**诚实标注**（9.1）；PR-5 下一轮 |
