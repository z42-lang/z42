# Tasks: 跨包 struct 值语义（P4a）

> 状态：🟢 已完成 | 创建：2026-08-12 | 完成：2026-08-12

## 进度概览
- [x] 阶段 1: 跨包 IsStruct 传播（编译器 4 文件）
- [x] 阶段 2: 跨包 golden 验证（复现→修掉崩溃）
- [x] 阶段 3: 验证 GREEN + 文档同步

## 阶段 1: 跨包 IsStruct 分类修复（单点）
- [x] 1.1 `z42.semantics/ImportedSymbolLoader.z42`（~:88）：`nct.IsStruct = !cl.HasBase`（根因修复，单行；复用既有 HasBase 编码）
- [x] 1.2 确认 `StructLayout.BuildFromSymbols` 对 imported struct 重算布局（无需改，依赖 1.1 分类）
- [x] 1.3 **bootstrap 约束（实测抓到，D1 定案）**：初版给 `ExportedClassZ`（z42.ir/stdlib）加新 `IsStruct` 字段 + z42c 源立即用 → `xtask test bootstrap` 报 `E0401: no field IsStruct`（axis ② stdlib API 越界，上一 nightly z42.ir 无此字段）→ 改用既有 `HasBase`（`!cl.HasBase` 精确等价 isStruct），零越界一个 nightly 落地

## 阶段 2: 跨包 golden 验证
- [x] 2.1 `src/tests/cross-zpkg/struct_cross_pkg/`（NEW）：A 定义 `struct Point`/`struct Line`，B 用（构造/字段/方法 Sum/传参 copy-in Bump/返回/`q=p` 值独立/嵌套 `line.a.x`）+ expected
- [x] 2.2 warm 建 A+B → 跑 B interp+jit EXIT=0（实测输出 `1 3 5 5 42 105 5 1 4 100 3` 全对，崩溃已修）

## 阶段 3: 验证 GREEN + 文档
- [x] 3.1 `cargo build --release` + `cargo test --lib` 全绿（runtime 未改，gate 过即可）
- [x] 3.2 `xtask test`（不传 Z42_HOME）全 stage 绿 + **self-host 5/5 byte-identical**（z42c 改动惰性）
- [x] 3.3 `xtask test e2e --dir cross-zpkg` 绿（含新 struct_cross_pkg）
- [x] 3.4 `xtask test bootstrap`（改了 z42c → 确认上一 nightly 能编当前源，无越界）
- [x] 3.5 spec scenarios 逐条覆盖确认
- [x] 3.6 `docs/book/.../struct-value-semantics.md` 加「跨包 struct（P4a）」节 + 页头对齐
- [x] 3.7 `docs/roadmap.md` Deferred 索引更新（P4a 已落，剩 P4b 反射 / P5-B / B-radical）

## 备注
- **⚠️ 增量缓存踩坑（血泪）**：mid-build `git checkout` 还原 z42.ir（stdlib）源文件后，`build compiler`/`build
  stdlib` 增量构建可能残留不一致状态 → 假 `BrCond expects bool, got Null` 崩溃（并非分类逻辑 bug）。
  `!cl.HasBase` 与 4-file `cl.IsStruct` 行为逐位等价，却因增量缓存损坏假崩。**解=清干净重建**
  （`rm -rf artifacts/build/compiler artifacts/build/libraries artifacts/.cache` 后重建）→ stdlib 24/24 全建、
  cross-zpkg 10/10。判据：还原/切换 stdlib 源后遇诡异 codegen 崩，先清缓存重建再判是不是真 bug。
- **格式中立**（无 zbc/zpkg bump、无新指令）→ 无 fixture 重生、无两代自举、warm 全程本地验证。
- 主门 = `cross-zpkg` golden（复现崩溃并修掉），实测已验。
- P4b（struct 字段反射）拆为 follow-up change `add-struct-field-reflection`（D3）。
- 环境：worktree `/Users/d.s.qiu/Documents/codesigner-ui/z42-p4`（seed 0.37==源），分支 `add-crosspkg-struct-value-semantics`。
