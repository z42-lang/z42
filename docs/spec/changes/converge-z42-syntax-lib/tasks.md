# Tasks: converge-z42-syntax-lib（PR-A 纯搬迁 + 构建接线）

## 搬迁
- [x] `git mv src/compiler/z42c.core src/libraries/z42c.core`（含 tests/diag）
- [x] `git mv src/compiler/z42c.syntax src/libraries/z42c.syntax`（含 tests/{lexer,parser,decl,stmt,dump}）
- [x] 6 个测试单元 toml：`output_dir` build/compiler→build/libraries；补 `z42.test` 依赖（stdlib 测试
      谐波要求，对齐 z42.ir 先例）；修注释内旧路径

## 构建接线
- [x] `src/compiler/z42.workspace.toml`：`default-members` 去 z42c.core/z42c.syntax，更新拓扑注释
- [x] `src/libraries/z42.workspace.toml`：`default-members` 末尾加 z42c.core/z42c.syntax（core→syntax 序）
- [x] `scripts/build/xtask_compiler.z42`：`_ensureBootstrapZ42Ir`→`_ensureBootstrapSelfDepLibs`，追加预建
      当前源 z42c.core→z42c.syntax 进 build-libs（破轴④环，不 warm-skip）
- [x] `.github/workflows/jit-fixpoint-check.yml`：成员表 5→3（前端移出 src/compiler workspace）；label 更新

## 未改（验证过自适应）
- ci-bootstrap 两代自举：每代先 `build --workspace`(stdlib，现含 z42c.core/syntax) 再建 src/compiler →
  前端先于后端进 flat，天然覆盖；快路径走 xtask（含破环）。
- `_compilerMembers`/`_stdlibList` 派生自各 workspace default-members → build/test/package/bootstrap-check
  全部自适应。`_assembleAllLibs`（stdlib dist + compiler members）：前端从 stdlib dist 供。
- `bench-pr.yml`：glob 拷 stdlib+compiler dist，自适应（非 required perf gate）。
- packaging：driver 从 libs/ 解析 z42c.core/syntax（与 z42.ir 同机制）；`_compilerMembers` 缩 → programs/z42c/
  只放 semantics/pipeline/driver。

## 文档
- [x] `docs/design/compiler/self-hosting.md`：目录树（前端下沉 src/libraries）；轴 ④（rename + 追加前端预建）
- [x] `src/compiler/README.md`：子包表（后端 3 包 + 已下沉共享库表）
- [x] `src/libraries/z42c.core/README.md` + `z42c.syntax/README.md`：加"位置"说明
- [x] 陈旧注释：`scripts/packages.toml`、`scripts/package/xtask_package_desktop.z42`（z42i 限制收窄）

## 验证（GREEN 交 CI —— 本地不可验：种子墙 + z42vm 退出期挂起）
- [ ] CI `ci-bootstrap` 两代自举 + verify（linux/mac/win）
- [ ] CI `test-host`×4（含 test compiler 后端 3 包 + test stdlib 含 z42c.core/syntax 单测 + jit）
- [ ] CI `jit-fixpoint-check`（compiler 后端 3 包 interp==jit）
- [ ] 零格式 bump（未动 zbc/zpkg writer）
