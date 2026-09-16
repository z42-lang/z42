# check-constraints-all-type-refs — where 约束满足性在所有类型引用位校验（H1 + H2）

> 状态：**DRAFT**（lang 语义变更，需 User 裁决后进 IMPL）
> 来源：2026-09-16「推进泛型表达力」gap 扫描 A 组（约束执行完整性维度）

## 问题

z42 的泛型 `where` 约束满足性校验今天**只在两处**触发：

1. `new T<...>()` 构造路径 —— `ConstructTyper.z42:181` 调 `ConstraintChecker.Check`
2. 泛型方法调用 —— `MemberResolver.z42:572/617` 调 `CheckMethod`

**H1（真洞）**：其余**所有**类型引用位——字段类型 / 属性类型 / 形参类型 / 返回类型 / 基类列表 /
`cast(as)` / `is` / `default(T)` / `typeof(T)` / 局部变量声明 / catch——解析出 `Z42InstantiatedType`
后**从不调 `Check`**。根因：这些位置统一走 `TypeChecker._chkTypeRef`（choke point），而它只做
access + deprecated + 跨包重复校验，**不接约束校验**。

后果：`Box<T> where T:INumber` 里 `Box<string>` 出现在字段/参数/返回等位置**零诊断**。运行期也不
兜底（`generics.rs` 的 `validate_type_arg_constraint` 只在反射 `MakeGenericType` 路径调用）。

**H2（真洞，同族）**：`ConstraintChecker.Check → _checkBundle` 把类型实参当**叶子**比对，**不递归**。
`new List<Box<string>>()` 只校验 `List` 的型参 bundle，**从不对内层 `Box<string>` 再 `Check`** ⇒
即便走了 `new`，嵌套泛型内层的约束违反也逃逸。

## 活跃受害者

first-party 源里带约束的泛型类是**核心集合**（`Dictionary<TKey,TValue> where TKey:IEquatable`、
`HashSet<T> where T:IEquatable`、`SortedSet<T> where T:IComparable+IEquatable`、`PriorityQueue`、
`LinkedList`）。stdlib 内这些类的实参都是 `int`/`string`（合法），故**今天无已触发的误编**；但任何
下游写 `HashSet<非IEquatable> f;` 或 `void g(Dictionary<BadKey,V> d)`（不经 `new` 位）即静默通过 =
**语言健全性洞**。属「闭洞立门」型（同 #604/#651/#636），修的是「编译器漏拒非法程序」这一根本正确性。

## 设计

### D1 — 挂载点：`_chkTypeRef` 单一 choke point

把 `Check` 接进 `TypeChecker._chkTypeRef`（TypeChecker.z42:45）：解析出的 `t is Z42InstantiatedType`
时调 `this._constraints.Check(env.Symbols, t as Z42InstantiatedType, sp)`。`_chkTypeRef` 已覆盖
new/var/cast/is/as/typeof/default/catch/泛型实参/字段/属性/参数/返回/基类列表全部引用点（其注释自陈
覆盖面），是天然正确的归属。

### D2 — 避免 `new` 位双报

`new` 路径今天：`ConstructTyper` 先调 `_chkTypeRef(t, env, n.Type.Span)`，**再**单独调 `Check`。
D1 接通后 `_chkTypeRef` 已含 `Check` ⇒ `new` 位会双报。

**方案**：删掉 `ConstructTyper` 里普通 `new` 的显式 `Check` 调用（`_chkTypeRef` 已覆盖，且用的正是
`n.Type.Span`），**只保留 target-typed `new()`（`n.Type == null`）分支的显式 `Check`**——那条路
`_chkTypeRef` 不会被调（它需要 `n.Type.Span`），靠显式调用 + `n.Span` 回落。这样零重复、零遗漏。

> 待核实（IMPL 阶段）：`_chkTypeRef` 在普通 `new`（`n.Type != null`）时确实被调、span 一致。

### D3 — H2 递归

在 `ConstraintChecker.Check` 内部，对每个 `inst.TypeArgs[i]` 若是 `Z42InstantiatedType` 则递归
`this.Check(symbols, arg, sp)`。这样 `_chkTypeRef` 只需对最外层解析出的类型调一次，内层由递归覆盖
（`List<Box<string>>` 作字段类型 ⇒ `_chkTypeRef` 拿到整体 → Check List → 递归 Check Box<string>）。

### D4 — 归一风险已被 `new` 位证明可用（非本变更风险）

`_satisfiesInterface`/`_satisfiesBase` 对 `int↔Int32`/`string↔String` 的归一**今天已在 `new` 位
运行**（`new Dictionary<int,int>()` 已走这条），stdlib 能编即证明归一正确。故爆炸半径**不来自归一
假阳性**，只来自「某非-new 类型引用位用了真违反约束的具体实参」。

### 爆炸半径探针（IMPL 前必做，数据见下）

按 #604 手法：直接发真诊断 + `rm .cache` 全量 `build stdlib` + `build compiler` + `build test`，
命中列出 file:line 供分类（真违反 → 修那段代码；归一假阳性 → 修 checker）。

**探针结果（2026-09-16，nightly 冷种子 `b722fb40`，零 skew）**：

*体内位（`_chkTypeRef`）+ H2 递归*：
- **harness 能报错已证**（阳性对照必做，[[verify-before-claiming-done]]）：探针包 `HashSet<NoEq>` 局部
  变量（体内位）→ 精确 `E0402: ... NoEq ... does not satisfy constraint IEquatable on HashSet`；
  `List<HashSet<NoEq>>`（H2 嵌套）→ 同样 E0402。
- **全仓零命中、零爆炸半径**：`build stdlib`（25 库）+ `build compiler` gen2（z42c 自建，含修正递归
  下钻所有嵌套泛型）→ `does not satisfy constraint` 命中 **0**。
- ⚠️ **H2 递归必须无条件先做**：初版把递归加在 `Check` 末尾，被 `if (!HasConstraints) return;` 早退
  跳过（外层 `List` 无约束 → 内层 `HashSet<NoEq>` 逃逸）。移到 `Check` 开头无条件递归后 line 8 才报。
- ⚠️ **`build test` 不能当探针 harness**：golden 走 `--emit-zbc`，丢弃全部诊断（#530 记过的
  「探针 harness 不会说话」）。有效 harness = `z42c build` / `build stdlib` / `build compiler`。

*声明位（字段/属性/参数/返回/基类）*：链路核实发现比「单一 choke point」复杂——
- `SymbolCollector._chkTypeRefT` 在**收集期**跑，约束尚未 resolved（`ConstraintChecker.Resolve` 在
  `CollectAll` 之后）⇒ 接不进去，需**绑定期独立 pass**。
- 已解析符号 `ct.Fields`/`ct.Methods` 含**继承展平**成员（走它们 → 跨类双报）；须走 AST `c.Members`
  的**自有**声明。
- 基类/接口位类型实参**未存**（`Z42ClassType` 只存 `BaseName`/`InterfaceNames` 字符串）⇒ 基类位
  须从 AST `c.Bases` **重解析**（带类型参上下文）。
- ⇒ 声明位是一个独立的、需自己探针的 pass（own-vs-inherited + 重载 + 基类重解析），非 body 的简单扩展。

*声明位（新 pass `ConstraintChecker.CheckDeclTypeRefs`，User 裁「完整 H1」后实现）*：
- 实现：绑定期新 pass 走 AST **自有声明**（`c.Members` 字段/属性/索引器/方法 + `c.Bases`），类型上下文
  （类型参 + 方法型参）逐字镜像 `SymbolCollector._methodSymbol`，`ResolveTypeP` 重解析后交 `Check`。
  挂在 `TypeChecker.Infer` 的 Resolve 之后（约束此时已 resolve/hoist）。
- **harness 全位置能报错已证**：探针包各位置精确报 E0402——字段(12,28) / 属性(14,27) / 参数(17,23) /
  返回(18,12) / 字段内嵌套 H2(21,34)。
- **D2 不双报已证**：`HashSet<NoEq> x = new HashSet<NoEq>()` 报 2 条但是**两个不同引用位**（局部标注
  col15 + new col37），`new` 自身只 1 条（走 _chkTypeRef，ConstructTyper:181 改成仅 target-typed 触发）。
- **声明位全量爆炸半径**：完整变更（体内位+H2+声明位）`build stdlib`(25 库)+`build compiler` gen2
  → `does not satisfy constraint` 命中 **0**。零假阳性、零 stdlib 潜在违反、零字节漂移（纯诊断不回灌发射）。

## 诊断码

复用既有 `DiagnosticCodes.TypeMismatch`（E0402，`_err` 现用的码）——不新增码。约束不满足的语义与
既有 `new`/方法调用位一致，同码同文案（`type argument \`X\` for \`P\` does not satisfy constraint
\`C\` on \`O\``）。**无格式 bump**（纯编译期诊断，不回灌发射）。

## 验证记录（IMPL 后）

- **单元门** `constraint_typeref_tests.z42`：14 条（体内位/cast/字段/属性/参数/返回/自由函数/H2×2 +
  各 satisfied 正例 + 型参转发不误报）全过。`test compiler` ✔、自举不动点 **3/3 gen1==gen2**。
- **退回对照（决定性）**：同时禁用三处 hook 重建 → **9 条负例全 FAIL**（body/decl/H2 各位置），
  **5 条正例全 PASS** ⇒ 门有判别力、非空门；恢复后 test compiler 全绿。
- **完整 GREEN（interp）**：`✔ test` 全 stage 绿（含 lines / walkers / e2e），5m20s。
- **jit + bootstrap**：`cross-zpkg --mode jit` 58/0 ✓、`test bootstrap` NO boundary violation ✓、
  `stdlib --mode jit` —— 首跑 1 文件 `http_server_threaded` 失败（`BrCond expects bool, got Null`），
  **查明是并发会话争端口的已知 flake**（[[concurrency-null-thread-flake]]；同机 `z42-lang/wt-gcinc`
  会话正并发狂跑 z42.net HTTP 测试），隔离重跑 z42.net jit **全过**（interp GREEN 亦已通过该测试）。非本变更回归。

## User 裁决

- **诊断码**：复用 E0402（User 已选完整 H1，诊断码复用既有约束码，无新码、无格式 bump）。
- **scope**：User 裁「完整 H1」= 体内位 + H2 + 声明位（已全部实现）。

## 原「待 User 裁决」（已定）

1. **诊断码**：复用 E0402（推荐，语义一致）还是新码（如 E0464，对称 #530/#604 的专码房规）？
2. **探针若命中真违反**：就地修那几段 first-party 代码（属本变更），还是先报告再定？
3. **H2 递归深度**：无限递归（`Box<List<Set<Bad>>>`）还是只下钻一层？（推荐无限——`Check` 幂等、
   嵌套有限，无终止风险。）

## 验证计划

- 单元门（`z42c.semantics/tests/` `SemanticDump`）：每个引用位（字段/参数/返回/基类/cast/is/
  default/typeof/局部）各一条负例 + H2 嵌套负例；退回对照精确变红。
- 运行期 e2e：纯诊断变更、零发射变化 ⇒ 正确路径由现有全套 stdlib/e2e GREEN 覆盖，负例编不过无法跑
  （同 #604/#651 scope 裁决，不新增运行期 e2e）。
- 完整 GREEN（interp）+ `stdlib --mode jit` + `cross-zpkg --mode jit` + `test bootstrap`。
- 自举不动点 3/3 gen1==gen2（无发射变化由构造保证）。
