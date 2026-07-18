# Proposal: partial 类型——单类型跨多源文件定义

> `lang` 类变更（新关键字 `partial` + 新类型声明规则）→ 完整流程（阶段 1–9）。
> 占用 `compiler` 单锁。**当前 `compiler` 被 `add-indexed-zpkg-min-patch` 持有 → 本 change 排队**
> （ACTIVE.md 已登记为排队中；锁释放后进阶段 6.5）。

## Why

1. **User 需求（2026-07-08）**：吸取 C# `partial` 的优势、规避其缺点，让**一个类型的定义拆到多个
   源文件**。主用例即 roadmap 0.4.4 已列的 CoreCLR `Interop.Unix.z42` / `Interop.Windows.z42`
   多平台抽象拆分。
2. **必须与既有增量编译共存**：z42c 是**文件级增量**（1 源文件 ↔ 1 IrModule ↔ 1 cache 条目，
   源哈希不变即跳过重编，`add-file-level-incremental` 对账器 47/47 byte-identical 实证）。
   partial 让类型跨文件，直接顶在"一文件一编译单元"假设上——方案必须保住"源没变就不重编"。
3. **物质基础已就位**：
   - zbc `TYPE` record 是**单条、内联全字段+全方法、有序**结构（VM/loader 只认完整类型）。
     只要**编译期**把碎片合并成一条完整 record，格式/loader/VM 全无感 → **零格式 bump**。
   - 增量失效闭包的"定义名属主"本就是**多所有者链表**（`OwnerNodeZ`，注释直书"partial 风格"）。
   - `record`/`struct`/`interface` 在 parser 阶段已统一降级为 `ClassDecl`（`Kind` 区分），
     partial 支持一次覆盖四种类型种类。

## What Changes

1. **新关键字 `partial`**（类型修饰符 + 方法修饰符）。
2. **partial 类型（class/struct/record/interface）**：同一类型可由多个 `partial` 声明拼成；
   全部声明必须同包 + 同 namespace，且**每个都写 `partial`**（缺一即编译错误）。
3. **编译期合并（方案 A）**：`SymbolCollector` 把同名 partial 碎片的字段/方法并入同一
   `Z42ClassType`（按**项目相对路径 Ordinal 序**再按声明序，字节确定）；`IrGen` 只让**主碎片**
   发一条完整 `TYPE` record（成员取自合并后的 `Z42ClassType`），其余碎片只发自己的方法体。
   → **不改 zbc/zpkg 格式、不改 Rust loader、不 bump 格式版本**。
4. **partial method（C# 9+ 干净形态）**：声明与实现可分处两碎片；**任意返回类型 + 允许访问修饰符
   + 允许 out**；无实现时整体擦除（不发桩、调用点消解）。**不采用旧版 void-only 那套规则**。
5. **增量共存**：失效闭包对同一 partial 类型的碎片文件**显式互连成团**（复用多所有者链表）——
   改任一碎片 → 整组碎片一起重编、一起重发合并 record；**非 partial 文件完全不受影响**，
   "源没变即跳过"保持不变。
6. **自举分阶段**：阶段 1 只落"支持"（z42c 认识 `partial`，但 z42c/stdlib 源自身不使用）→
   `xtask test bootstrap` 确认上一版 nightly 仍能编当前源 → 发 nightly → 之后才在 z42c/stdlib/
   examples 中**使用** partial。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | 新增 `Partial` token 常量 |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | `_initKeywords()` 注册 `partial` |
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | 类型声明 + 方法声明接受 `partial` 修饰符 |
| `src/compiler/z42c.syntax/src/Decl.z42` | MODIFY | `ClassDecl.IsPartial`、`MethodDecl.IsPartial`/`HasBody` 位 |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 碎片合并（stub 指同一 `Z42ClassType` + 成员按稳定序并入）；基类/主构造器单碎片校验；接口并集；重复成员冲突检测；partial method 声明↔实现匹配 |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | partial 归属标记 / 有序备份按合并序追加（如需） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | `_classDesc` 成员取自合并 `Z42ClassType`；仅主碎片发 `TYPE` record；partial method 擦除 |
| `src/compiler/z42c.semantics/src/ExportedTypeExtractor.z42` | MODIFY | 导出合并后的完整类型一次（跨包消费方零改动） |
| `src/compiler/z42c.pipeline/src/IncrementalBuild.z42` | MODIFY | partial 碎片组显式互连边（联动失效） |
| `src/compiler/z42c.syntax/tests/parser/partial/` | NEW | partial 声明解析单测（正常 + 缺 `partial` 报错） |
| `src/compiler/z42c.semantics/tests/collect/partial_merge/` | NEW | 碎片合并 / 冲突检测 / 顺序确定性单测 |
| `src/tests/partial-types/` | NEW | partial 跨文件端到端（build + run）golden |
| `src/tests/partial-types/incremental/` | NEW | 改一碎片增量重编 == 全量（字节）对账 |
| `examples/partial.z42`（或多文件目录） | NEW | partial 示例 |
| `docs/book/src/language/partial-types.md` | NEW | partial 机制页（语法 + 合并语义 + 增量交互，含 mermaid） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |
| `docs/design/language/grammar.peg` | MODIFY | 语法加 `partial` 修饰符产生式 |
| `docs/roadmap.md` | MODIFY | 0.4.4 partial 状态更新 |
| `src/compiler/z42c.syntax/README.md` | MODIFY | 六段同步（如触及入口） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 六段同步（合并机制入口） |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 排队登记 / 占锁 / 释放 |

**只读引用**：`src/compiler/z42c.ir/src/IrModule.z42`（`IrClassDesc` 形态）；
`src/compiler/z42c.project/src/CacheStore.z42`（增量 cache 条目 SoT）；
`src/runtime/src/metadata/zbc_reader.rs`（确认 TYPE record 解码不受影响）；
`.claude/rules/common-pitfalls.md`（合并顺序确定性硬约束）；
`.claude/rules/bootstrap-seed.md`（分阶段引入纪律）。

## Out of Scope

- **跨包 partial**：不做。跨包给类型加成员用已有 `impl Trait for Type`。
- **加载期合并（方案 B）**：不做。不改 zbc/zpkg 格式、不改 Rust loader、不 bump 格式版本。
- **单碎片增量（body-only 编辑不牵连兄弟碎片）**：v1 不做，接受 partial 组联动失效；
  若未来有需要，记 Deferred（合成 type-metadata 模块方案）。
- **partial 与泛型的交互**：沿用现有 `ClassDecl` 对泛型的处理，不额外为 partial 扩展。
- **v1 只做顶层类型 partial**（2026-07-19 定，见 design D9）：partial 外层**含**嵌套类允许（嵌套按声明
  碎片落位）；但**嵌套类自身 partial**（`Outer.Inner` 跨碎片拆）**v1 不做 → 报错 + Deferred**——原因是嵌套
  发射链路本身未接通，非机制受限（接通后同 min-path 规则递归解禁）。

## Open Questions

- [ ] **自举能力版本号**：bootstrap-seed.md 提及 z42c 应带"语法能力版本号"，但当前**未实现**
      （只有 Main.z42 一个版本串）。本 change 是否顺带引入该数字并 +1？还是维持现状、
      仅靠 `xtask test bootstrap` 兜底？→ design D-boot 待裁。
- [x] **主碎片选定规则**（2026-07-19 定）：**路径 Ordinal 最小的碎片，单一规则**，砍掉"基类/ctor 碎片
      优先"特例（合并 record 内容与发自哪个碎片无关）。→ design D3 已改。
- [x] **indexed 发布态方法体落点**（2026-07-19 定）：**默认散留各碎片 zbc**（VM `merge_modules` 按名合并，
      散/合加载后逐字节相同 → 散着零改动 + 保住 cache 字节拷贝）。→ design D8。
- [ ] **partial method v1 是否真做**：User 倾向"照 C# 全做"，但它是唯一被点名规避的 C# 缺点；
      已定"做但只采 C# 9+ 干净形态"。design D5 复述该裁决，实施前最终确认。
