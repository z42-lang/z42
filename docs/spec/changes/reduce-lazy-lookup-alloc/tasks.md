# Tasks: 消除 lazy 查找的每调用 HashSet 克隆

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06
**变更说明：** `VmContext::try_lookup_function` / `try_lookup_type` 改用 loader 的
`newly_loaded` 暂存缓冲（`clear` + `mem::take`）来检测本次 resolve 新加载了哪些 zpkg，
替代原先**每次调用都克隆整个 `loaded_zpkgs: HashSet<String>` 再 diff**。
**原因：** profile z42c 自编译（真实跨包 workload）发现 `VmContext::try_lookup_function`
是 interp 侧 malloc 最大单一来源（219 采样 ≈ 4.4% 总时间）——它每次跨包查找都
`loaded_zpkgs.clone()`（全量 String 集）+ 事后 `difference().cloned()`，只为在**罕见**的
"这次真加载了新包"时 fire `ModuleLoaded` 事件。绝大多数查找不加载新包，却每次付一份全集克隆。
**文档影响：** 无对外行为变更（事件语义不变、结果不变）；纯内部分配优化，无需 book/README。

## 机制
- `LazyLoader` 新增 `newly_loaded: Vec<String>` 字段 + `mark_zpkg_loaded(name)` 助手
  （`if loaded_zpkgs.insert(name.clone()) { newly_loaded.push(name) }`，只记**真新增**，
  与原 `difference` 语义一致）。
- 3 处 `loaded_zpkgs.insert(...)`（文件 load 预插入 / in-memory 模块 / 包 canonical 名）
  改走 `mark_zpkg_loaded`。
- 两个 `try_lookup_*`：`loader.newly_loaded.clear(); let r = resolve(...);
  let newly = mem::take(&mut loader.newly_loaded);`。常态无加载 → 缓冲空 → `mem::take`
  零 cap Vec → **零分配**；有加载（罕见）→ 排空得到新包名，语义等价。

## 任务
- [x] 1.1 `metadata/lazy_loader.rs`：加 `newly_loaded` 字段 + 构造初始化 + `mark_zpkg_loaded` 助手
- [x] 1.2 `metadata/lazy_loader.rs`：3 处 `loaded_zpkgs.insert` → `mark_zpkg_loaded`
- [x] 1.3 `vm_context.rs`：`try_lookup_function` / `try_lookup_type` 换 clear+`mem::take`
- [x] 1.4 测量：z42c 编译 big.z42 应较基线（interp 22.1s / jit 22.4s，无 mimalloc）下降
- [x] 1.5 GREEN：`cargo test --lib`（含 lazy_loader / host zpkg 单测）全过；e2e 抽样 interp==jit
- [x] 1.6 回归测试：现有 lazy_loader / cross-zpkg 单测覆盖事件语义（ModuleLoaded 仍正确）

## Out of scope
- 与本次正交的其它分配点（interp vcall arg Vec、push_frame）——后续独立 change
- mimalloc 分配器（独立 change，已开 PR #128）

## 备注
- 事件语义严格保持：`mark_zpkg_loaded` 只在 `insert` 真新增时 push，等价原 `difference`；
  `clear()` 保证只报本次 resolve 的加载（不含其它路径遗留）。
