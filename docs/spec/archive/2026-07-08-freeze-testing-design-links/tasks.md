# Tasks: freeze-testing-design-links

> 状态：🟢 已完成 | 创建：2026-07-08 | 完成：2026-07-08 | 类型：docs（直接实施）

**变更说明：** 落地 `docs/xtask_review.md` §4.4（GREEN 清单最后一处矛盾）+ §4.6（断掉 workflow 页
指向已冻结 design/testing/testing.md 的冗余"详见"链——所引内容已内联在 workflow 页）。
**原因：** design/testing 声明冻结（doc-system D2）却仍被 workflow 页"详见"引用而保持存活+陈旧；
testing/README:29 的 stage 名仍是旧的 `vm/lib`。
**文档影响：** 本身即文档。只动 live 文档，不动 design/（冻结）本体、不动 archive/。

## 范围
- [x] 1.1 §4.4：`testing/README.md:29` GREEN 门禁串联 stage 名 `vm/cross-zpkg/lib/compiler`
      → `e2e/cross-zpkg/stdlib/compiler`（对齐同文件 :39-40 与 workflow.md 阶段8）。
- [x] 1.2 §4.6：断掉指向 design/testing 的冗余"详见"链（内容已内联在各 workflow 页）：
      - `vm-tests.md:62`「测试目录组织」段（页内已有目录树）
      - `vm-tests.md:75`「归属规则」段（页内已述规则）
      - `changed-only.md:30`「增量测试」段（页内已有映射表；真实 SoT = 代码 _mapFile）
- [x] 1.3 归档。

## Out of Scope（留后续，需更多判断/迁移）
- `stdlib-tests.md:43`（R 系列实施进度）/ `unit-tests.md:36`（编写新测试）/ `platform-tests.md:4`
  （cross-platform-testing.md）——所引可能含 workflow 页未覆盖的专门内容，逐条迁移需单独判断。
- design/testing/testing.md（~1900 行）整体迁往 book = 项目级 design→book 迁移（doc-system D2），非本次。
- §4.4 GREEN 清单的完整单一 SoT 重构——与 CLAUDE.md「GREEN 定义在 workflow.md 阶段8」耦合，
  现状已一致（各处或复用一致 stage 名、或指回阶段8），不做进一步 SoT 权属调整。
