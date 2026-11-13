# 桌面 SDK 验证

## 职责

`package sdk --verify` 的执行器与 packages.toml 自检。验证刚组装出的桌面 SDK，覆盖安装清单、发行包命令行与 apphost 夹具，以及 interp / jit golden。
生产打包实现位于 `scripts/src/package/`；这里的代码不参与设备或浏览器包的构建。

## 功能索引

| 功能 | 入口 |
|---|---|
| 验证本次 SDK 的发行包夹具与 golden | `test/sdk/verify.z42` 的 `_packageVerify` |
| 编译并执行发行包 golden，同时检查退出码与输出 | `test/sdk/goldens.z42` |
| 回归真实退出 0 / 7、输出不符与编译失败 | `test/sdk/goldens_selfcheck.z42` 的 `_selfcheckDistGoldens` |
| 验证 Cargo 消息转发、元数据透传与子进程退出码 | `test/sdk/cargo_capture_selfcheck.z42` |
| 校验 Cargo 产物路径和生产方 Rust std 的拷贝边界 | `test/sdk/cargo_artifacts_selfcheck.z42` |
| 校验包配置、组件安装与发布归档 | `test/sdk/package_selfcheck.z42` 的 `_pkgSelfCheck` |

## 如何测试验证

```bash
.github/ci/xtask package sdk --verify
```

CI 在四个桌面 host package 中执行同一入口。测试执行器与私有发行包夹具改动按目录职责选择这些 job，不需要逐用例声明平台或 tier。

## 关联文档

- [打包机制](../../../../docs/internals/src/devinfra/packaging.md)
- [CI 拓扑](../../../../docs/internals/src/devinfra/ci.md)
- [声明式夹具](../../../../docs/internals/src/devinfra/fixture-harness.md)

`test/sdk/payload_selfcheck.z42` 验证发布闭包对账：忽略 dist 中无关包，检查传递依赖和原生库，允许外部 SDK 依赖，并终止循环依赖。由 `package sdk --verify` 的自检阶段执行。
