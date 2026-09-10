# enforce-test-attr-placement — 任务分解

> 状态：🟢 已完成 | 完成：2026-09-11

> 规范：[proposal.md](proposal.md)（Why + Scope）· [specs/test-attribute-placement/spec.md](specs/test-attribute-placement/spec.md)（What，R1–R7）· [design.md](design.md)（How）。
> **单批交付，无分期。**
> 通用 usage 框架已存档在 [design-full-usage-framework.md](design-full-usage-framework.md)，不在本次范围。

**改动面**：1 个新方法（约 60 行 z42）+ 3 行挂载 + 1 个 fixture 重冻结 + 文档修正。
**不新增诊断码、不改语法、不改 zbc 格式。**

---

## T0 审计（已跑）

- [x] 全仓矩阵 → design §1.3：`[Test]` 3807 合法 / **2 处实例方法违规**；
      `[Benchmark]` 66、`[Setup]`/`[Teardown]` 各 1，全合法。
- [x] 自举自查：`src/compiler/**` + `src/libraries/**` 零违规。

脚本（建议落 `scripts/audit/audit_attrs.py`）。**同行写法 `[Record] class Pair(...)` 与独占一行两种都要认**
——第一版只看下一行，把 78 处同行 `[Record]` 全归错了：

```python
import re, sys, pathlib, collections
NAMES = {"Test","Benchmark","Setup","Teardown","Skip","Ignore","ShouldThrow","Timeout",
         "Native","Suppress","Deprecated","Record","Usage"}
ATTR = re.compile(r'^\s*\[([A-Za-z_][A-Za-z0-9_]*)(?:<[^\]]*>)?(?:\(.*?\))?\]\s*')
TYPEDECL = re.compile(r'^(?:(?:public|private|internal|protected|sealed|abstract|static|partial)\s+)*'
                      r'(class|struct|interface|enum)\b')
MODS = re.compile(r'^((?:(?:public|private|internal|protected|sealed|abstract|static|partial|override|virtual|extern|const|readonly)\s+)*)')

def classify(decl, nested):
    ds = decl.strip()
    t = TYPEDECL.match(ds)
    if t: return t.group(1)                      # class / struct / interface / enum
    mods = MODS.match(ds).group(1)
    is_static = 'static' in mods
    body = ds[len(mods):]
    if '(' in body and body.split('(')[0].strip() and not body.split('(')[0].strip().endswith('='):
        head = body.split('(')[0].strip()
        # ctor: 无返回类型（单个标识符）且首字母大写、且嵌套
        if nested and len(head.split()) == 1: return 'ctor'
        if not nested: return 'free-fn'
        return 'static-method' if is_static else 'instance-method'
    if body.rstrip().endswith(';') or re.match(r'^[A-Za-z_][\w.<>\[\]]*\s+\w+\s*(=|;)', body):
        return 'static-field' if is_static else 'field'
    if '{' in body and ('get' in body or 'set' in body): return 'property'
    return 'other:' + ds[:40]

m = collections.Counter(); sample = collections.defaultdict(list)
for p in sorted(pathlib.Path('src').rglob('*.z42')):
    lines = p.read_text(errors='replace').splitlines()
    for i, l in enumerate(lines):
        mt = ATTR.match(l)
        if not mt or mt.group(1) not in NAMES: continue
        rest = l[mt.end():].strip()
        if rest and not rest.startswith('//'):       # 同行声明：[Record] class Pair(...)
            decl, ln, nested = rest, i, (len(l) - len(l.lstrip())) > 0
        else:                                        # 独占一行：往下找第一条真声明
            j = i + 1
            while j < len(lines):
                s = lines[j].strip()
                if not s or s.startswith('//') or ATTR.match(lines[j]): j += 1; continue
                break
            if j >= len(lines): continue
            decl, ln, nested = lines[j], j, (len(lines[j]) - len(lines[j].lstrip())) > 0
        k = (mt.group(1), classify(decl, nested))
        m[k] += 1
        if len(sample[k]) < 3: sample[k].append(f"{p}:{ln+1}  {decl.strip()[:72]}")
for k in sorted(m): print(f"{k[0]:12} {k[1]:20} {m[k]:5}")
print("\n--- rare / unclassified ---")
for k in sorted(m):
    if m[k] <= 6 or k[1].startswith('other'):
        for s in sample[k]: print(f"  {k[0]:10} {k[1][:24]:24} {s}")
```

---

## 实施

- [x] 1a. `HandlerRegistry.z42`：加 `IsTestKindAttr(name)`（4 名 kind 子集），并把既有
      `IsTestHandlerAttr`（8 名）改成复用它（纯重构，集合不变）—— design §3.1(a)。
- [x] 1b. `z42c.semantics/src/DeclEnforcer.z42`：加 `_passTestAttrEnforce` + `_teWalk` / `_teCheck` /
      `_teKindAttr` / `_teCode` / `_teShape` —— **完整伪码见 design §3.1(b)，可直接照抄**。
      - 递归覆盖：顶层 + `ClassDecl.Members`（含嵌套类型）+ `ImplDecl.Methods`；覆盖表见 §3.1(d)。
      - 五条规则 R1–R5（design §2），**全纯语法**，不依赖符号表。
      - **R1 违规后 `return`**（贴错位置别再刷屏）；R2–R5 各报一次。
      - 诊断锚在 **attribute 的 span**（`kind.Span`），不是方法 span——本 parser 的 decl span
        只覆盖起始 token，且一个方法可能贴多个 attribute。
- [x] 2. `SymbolCollector.z42` 三个挂载点各加一行，紧邻既有三个后缀 pass：
      `CollectWithImports`（`:60-62`）、`CollectAll`（`:173-177`）、`Collect`（`:198-200`）。
      三者是**互斥的公开入口**（一次编译只走其一）→ 不会重复报，与既有后缀 pass 同构。
- [x] 3. **⚠️ 相位约束写进代码注释**（design §3.2）：本 pass 必须在 `HandlerRegistry.RunAst` 之后——
      `BenchmarkDesugar` 会把**合法**的 form-2 `[Benchmark] void f(Bencher b)` 改写成零参 wrapper，
      在它之前查 R3 会让**全仓 66 处 benchmark 立刻全红**。
- [x] 4. 诊断：**复用已有的 E0911 / E0912 / E0915**（design §3.3），不新增码。
      这三个常量早已存在于 z42c.core（非新增）→ **可直接引用常量**，不必走"先用字面量"的 F2 规避；
      若种子过旧再退回字面量。消息模板照 design §3.3，**必须说清允许什么**。
- [x] 5. **修存量唯一违规**：`src/tests/zbc-format/with-tidx/source.z42` 两个方法加 `static`，
      按 [zbc-format/README.md](../../../../src/tests/zbc-format/README.md) 重新冻结
      `source.zbc` + `expected.json`。
      ⚠️ 字节级 golden：`git diff` 应**只有** `param_count` 1→0、`param_types` 置空、
      `is_static` false→true + 随之的字节偏移，逐行看懂才能合。

## 测试

- [x] 6. `src/compiler/z42c.semantics/tests/collect/test_attr_enforce_tests.z42`（NEW）——
      逐条覆盖 spec 的 R1–R7 场景（含嵌套类内、impl 块内、构造器、多码分派、R2+R3 并发报出）：
      `[Test]` 实例方法 / `[Test] int f()` / `[Test] void f(int x)` / `[Test] void f<T>()` /
      `[Test] void f();`。断言**码 + 消息含允许集**。
- [x] 7. **相位回归 golden**：form-2 `[Benchmark] void f(Bencher b)` **必须编译通过**
      （锁住第 3 条，防止将来有人上移 pass）。
- [x] 8. Positive 回归：3807 处 `[Test]` + 66 处 `[Benchmark]` + `[Setup]`/`[Teardown]` 一处不能报错。
- [x] 9. 门禁：`./xtask test` 全绿 + golden 不动点 + 自举通过。
      （⚠️ 别用整套 `cargo test`——会挂在上百个不可 kill 的 signal helper 上。）

## 文档

- [x] 10. `docs/design/compiler/error-codes.md#L139`：**修假陈述** —— E0911/E0912/E0915 的实施位置
      从已删除的 `src/compiler/z42.Semantics/TestAttributeValidator.cs` 改为
      `z42c.semantics/src/DeclEnforcer.z42`；**E0913/E0914/E0917 标注「未实现，跟进项」**
      （当前写"已启用"是假的）。
- [x] 11. `docs/design/testing/testing.md`：§编译期校验 指向新 pass。
- [x] 12. `docs/design/language/attributes.md`：`attribute-future-attributeusage` 留在 Deferred，
      加一行指向 [design-full-usage-framework.md](design-full-usage-framework.md)。

---

## 后续独立立项（design §4.6 —— 「谁的规则谁处理」的正解）

> 这两步**不属于本变更**（无 checkbox，避免被归档扫描误判为未完成任务）。做完后 `[Test]` 的校验
> 自然归 z42.test 且**自动生效**，本批的 pass 随 TestIndexBuilder 一起整体删除。
> 全景见 [`systematize-test-pipeline`](../systematize-test-pipeline/design.md) 的 S4 / S5。

**A. analyzer 随库携带**（→ 独立变更，不属本变更）（对标 Roslyn analyzer 随 NuGet 包分发）：让**依赖**能贡献 analyzer，
      消费方无需在 z42.toml 写 `[analyzers]`。破掉 opt-in 死结——**z42.test 已是每个测试包的依赖**，
      一旦支持即 **28/28 自动生效、零 manifest 改动**。
      现状：[`ManifestLoader._parseAnalyzers`](../../../../src/libraries/z42.project/src/ManifestLoader.z42#L94)
      是扁平显式列表，无传递/贡献概念。
**B. TIDX 退休**（→ 独立变更，不属本变更）：test 家族从内建 handler 变普通 store-meta attribute + 反射发现，把编译器里
      **12 个文件**的 test 知识撤出。已记录为"独立后续变更"
      （[HandlerRegistry.z42:15](../../../../src/compiler/z42c.semantics/src/HandlerRegistry.z42#L15)）。

## 跟进小项（不阻塞，按需）

| 项 | 码 | 为什么没进本批 |
|---|---|---|
| 孤儿修饰：`[Skip]` 无 `[Test]` | E0914 | 会让 TIDX 多一条凭空的 skipped entry，但**不崩** |
| `[ShouldThrow<E>]` 的 E 须继承 `Exception` | E0913 | **需符号表**判继承链 → 另一个相位，不是纯语法 |
| `[Timeout]` 值域（缺失 / ≤0）| E0917 | 实参语义，同上 |
| `[Native]`/`[Record]`/`[Deprecated]` 的位置约束 | — | 贴错不崩、只是静默无视；审计确认存量全合规 |
| 用户自定义 attribute 的位置约束 | — | 通用框架已存档；真有需求时写 `Analyzer`（design §4.4）|


---

## 完成记录（2026-09-11）

**GREEN**：`xtask test` **全 stage 通过**（e2e goldens / cross-zpkg / multi-exe / stdlib [Test] /
manifest targets / examples / compiler 含**自举不动点 gen1==gen2 3/3** / vscode-syntax / lines），
外加 `cargo test --test zbc_compat --test format_fixture_versions` 5/5。

**新增测试**：`z42c.semantics/tests/collect/test_attr_enforce_tests.z42` —— **22 例全绿**，
逐条覆盖 spec R1–R7（含嵌套类 / impl 块 / 构造器 / 多码分派 / R2+R3 并发报出 / 相位哨兵）。

**实施中的两处发现**（均已修正，写入 design §3.1）：

1. **嵌套类型会被访问两次** —— `NestedFlatten`（本 pass 之前跑）把嵌套类提升为顶层
   （改名 `Outer+Inner`）**但仍留在 `outer.Members`**。递归时必须跳过 `ClassDecl`/`EnumDecl` 成员，
   否则同一声明报重复诊断。与其它成员 pass 的既有约定一致。
2. **诊断限定名要还原源码拼法** —— 展平后 owner 是 `Outer+Inner`（元数据拼法），
   诊断面向源码作者，`_teShape` 里 `+` → `.`。

**存量修正**：`src/tests/zbc-format/with-tidx/` 两个方法加 `static` + 重冻结
（`source.zbc` 839→800 字节；`expected.json` 的 `param_count` 1→0 / `param_types` null / `is_static` true）。

**⚠️ 发现的 Scope 外问题（未动，待裁决）**：`src/tests/zbc-format/*/expected.json` **六份全部声称
`minor: 20`，而当前格式是 `minor: 38`**（落后 18 个 minor），且**全仓零消费者**
（README 的核心文件表也不列它们）。这是先于本变更就存在的孤儿产物。
建议独立变更处理：要么删（死文件、且正因为可信才误导人），要么纳入 `xtask build test` 的重生范围。
