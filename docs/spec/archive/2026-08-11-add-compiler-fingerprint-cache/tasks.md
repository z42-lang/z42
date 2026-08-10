# Tasks: 增量 cache key 加编译器语义指纹

> 状态：🟢 已完成 | 创建：2026-08-11 | 完成：2026-08-11 | 类型：fix（增量缓存正确性）

**变更说明：** 增量编译 cache 的失效判据（`.meta` / `package.meta`）当前只含
`源内容 SHA-256 + zbc/zpkg 格式 Minor`，**不含 z42c 编译器自身的语义指纹**。当编译器
codegen / 优化 / typecheck / lowering 行为变化，但源文件没变、且 zbc/zpkg 格式 Minor 没
bump 时，`ProbeFiles` 会命中旧 cache → **静默复用旧 .zbc 产物、不重编** → 产物与当前
编译器语义不一致。

**原因：** 修正增量缓存的正确性漏洞（todo#7「版本不同会重新编译」的 A 方案：手动语义指纹）。
自动聚合 z42c zpkg `build_id` 的 B 方案作为 follow-up 登记（见备注）。

**文档影响：**
- `.claude/rules/version-bumping.md` — 新增「编译器语义指纹 bump」小节
- `src/compiler/z42c.pipeline/src/CacheStore.z42` 顶部注释（meta 格式记述加 `z42c-fp` 行）
- `src/compiler/z42c.driver/README.md` — cache meta 头字段记述（若列了 pin 字段）

- [x] 1.1 `CacheStore.z42`：`MetaVersion` 2→3；新增 `CompilerFingerprint` 常量（初值 1）
- [x] 1.2 `CacheStore.Serialize` / `Parse`：写入 + 校验 `z42c-fp` 行（不符 → 条目作废）
- [x] 1.3 `CacheStore.SaveSrcList` / `LoadSrcList`：`package.meta` 同步 `z42c-fp` 行
- [x] 1.4 顶部注释同步 meta 格式（加 `z42c-fp <CompilerFingerprint>` 行）
- [x] 1.5 回归测试：`incremental_tests.z42` 加 fp 不符 → Parse 返回 null 用例（`test_cache_meta_fingerprint_pin`）
- [x] 1.6 文档同步：`version-bumping.md` 指纹 bump 纪律 + `project.md` 增量节 pin 字段（driver README 抽象层无需改）
- [x] 1.7 GREEN：`xtask test` 全 stage 绿（增量单元 13/13 含 `test_cache_meta_fingerprint_pin`；自举 5/5 gen1==gen2 byte-identical）

## 备注

**B 方案（follow-up，登记 roadmap Deferred）**：driver 经 `Z42_HOME` 解析自身
`programs/z42c/*.zpkg`，聚合已有 `build_id`（BLAKE3-128，`ZpkgWriter.z42:70`）成指纹入
cache key，令编译器一变即自动失效、无需人肉 bump。暂缓原因：多启动路径
（cold/warm/REPL/z42b/wasm）下 `Z42_HOME` 解析自身产物的验证面较大，发布前不值得压。
触发条件：0.4.x 尾 build-orchestration 阶段一并做。
