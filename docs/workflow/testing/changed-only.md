# 增量测试（只跑 git diff 影响）

dev 内循环加速：`./xtask test changed` 根据 git diff 把改动文件映射到测试命令，去重后执行最小集合。

## 命令

```bash
./xtask test changed                 # base = HEAD（unstaged + staged）
./xtask test changed main            # base = main（branch 全部差异）
./xtask test changed --dry-run       # 只打印计划，不执行
Z42_TEST_CHANGED_BASE=origin/main ./xtask test changed
```

## 文件 → 测试映射

| 改动 | 触发 |
|------|------|
| `src/libraries/<lib>/src/*` | `./xtask test stdlib <lib>` + `./xtask test e2e` |
| `src/libraries/<lib>/tests/*` | `./xtask test stdlib <lib>` |
| `src/runtime/src/*` / `Cargo.toml` / `build.rs` | `cargo test` + `./xtask test e2e` |
| `src/runtime/tests/*` | `cargo test` |
| `src/tests/cross-zpkg/*` | `./xtask test e2e --dir cross-zpkg` |
| `src/tests/*` | `./xtask test e2e` |
| `src/compiler/*` | `./xtask test compiler` + `./xtask test e2e` |
| `src/toolchain/*` | runner cargo test + `./xtask test stdlib` |
| `*.md` / `docs/**` / `.claude/**` | 不触发 |
| `*.workspace.toml` / `build.rs` / `scripts/xtask*.z42` | 全套 `./xtask test` |
| 其他 `src/**` | 全套 `./xtask test`（防御性） |

完整映射的唯一真相源是代码 `scripts/test/xtask_test_changed.z42` 的 `_mapFile`（本表是其快照）。

## 局限

- 不考虑跨文件传递依赖（改 stdlib 内部 helper 不会触发依赖该 helper 的 cross-zpkg 测试）
- workspace toml / build.rs / xtask 源码触发全套（粗粒度）
- 适合**内循环加速**；推送前仍跑 `./xtask test` 全套

## 实施

`./xtask test changed` 用 `git diff --name-only <BASE>` 收集 tracked changes + `git ls-files --others --exclude-standard` 收集 untracked，去重后 case 映射 + dedup 命令 + 固定顺序执行（compile → vm → stdlib → cross）。

source: `./xtask test changed`。
