# z42b 命令行夹具

## 职责

每个目录是一个最小工程。`expect.toml` 声明要执行的 z42b 命令及退出码、输出、产物断言，
由 `xtask test toolchain builder` 复用通用夹具引擎执行；新增常规用例无需修改 xtask。
涉及过期父包副本、`--reuse-parent` 或插件执行顺序的测试仍由 `xtask_test_targets.z42` 驱动。

## 工具与占位符

默认工具 `z42b` 使用开发树 VM、builder 与 stdlib，在用例目录执行。参数中的 `{case}` 是
用例暂存目录，`{vm}` 是开发树 VM 路径，`{rid}` 是当前宿主 RID。

夹具整棵树每轮重置后复制到 `artifacts/intermediate/toolchain/builder/tests/fixtures/z42b/`，
源码树不写入产物。`--list` 用例在构建前执行，验证不会创建 `artifacts/` 或 `dist/`。

## 功能索引

| 用例 | 验证内容 |
|---|---|
| `empty-glob`、`manifest-sections`、`orphan-source` | 编译失败的收尾、配置继承和未发现测试的错误 |
| `no-tests`、`dev-target-internal` | 无测试正常退出；只列目标、不编译执行 |
| `build-hooks`、`hook-*` | hook 输入、源集编译加载、依赖携带与插件构建失败 |
| `workspace-default` | 默认成员、依赖顺序与 `--workspace` 全成员构建 |

## 如何测试验证

```bash
xtask test toolchain builder --no-build
```

## 关联文档

- [声明式夹具格式](../../../../../../docs/internals/src/devinfra/fixture-harness.md)
- [测试用例组织规范](../../../../../../docs/internals/src/devinfra/test-layout.md)
