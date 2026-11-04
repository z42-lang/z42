# 桌面 SDK 验证

## 职责

`package sdk --verify` 的执行器与 packages.toml 自检。验证刚组装出的桌面 SDK，覆盖安装清单、发行包命令行与 apphost 夹具，以及 interp / jit golden。
生产打包实现位于 `scripts/package/`；这里的代码不参与设备或浏览器包的构建。

## 如何测试验证

```bash
.github/ci/xtask package sdk --verify
```

CI 在四个桌面 host package 中执行同一入口。测试执行器与私有发行包夹具改动按目录职责选择这些 job，不需要逐用例声明平台或 tier。

## 关联文档

- [打包机制](../../../docs/internals/src/devinfra/packaging.md)
- [CI 拓扑](../../../docs/internals/src/devinfra/ci.md)
- [声明式夹具](../../../docs/internals/src/devinfra/fixture-harness.md)
