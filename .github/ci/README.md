# CI 启动入口

## 职责

`xtask` 垫片用仓库 SDK 的 VM 与 libs 执行当前源码编译的 xtask。流程选择、GitHub 控制逻辑及其回归均由
`scripts/src/flow/` 与 `scripts/src/ci/` 实现；本目录不维护另一套路径分类或测试执行器。

## 基础用法

```bash
xtask flow plan --event-file "$GITHUB_EVENT_PATH" --out artifacts/ci-plan.json
xtask ci test-host --plan artifacts/ci-plan.json
xtask ci retry-runner --run-id <ID> --dry-run
xtask ci upload-release nightly <asset> [<asset> ...]
```

## 如何测试验证

`xtask check xtask` / `xtask check flows` 在本地和 CI 中执行同一套离线回归，覆盖真实 Git 基线、选路、计划过期、发布范围、
runner 恢复、上传重试、自举探针、golden 传递与安装器；无需构建当前产品。
`xtask bench check` 用入库 JSON 验证性能判定规则。完整门禁仍为 `xtask test`。

## 关联文档

- [流程配置与计划](../../docs/internals/src/devinfra/flows.md)
- [CI 拓扑与选择边界](../../docs/internals/src/devinfra/ci.md)
