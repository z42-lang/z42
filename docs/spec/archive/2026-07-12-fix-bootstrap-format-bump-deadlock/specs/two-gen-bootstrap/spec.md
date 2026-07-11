# Spec: 两代自举(格式-bump 引导)

## ADDED Requirements

### Requirement: 版本差 gate

#### Scenario: 无格式 bump(日常 push)
- **WHEN** ci-bootstrap 运行,下载种子的 zpkg minor == 当前源码 `ZpkgWriterZ.Minor`
- **THEN** 走现有单 VM 快路径,行为与本 change 前逐字节一致,无额外编译

#### Scenario: 格式 bump(种子落后一个 minor)
- **WHEN** 种子 zpkg minor < 当前源码 writer minor
- **THEN** 触发两代自举(Gen1 → Gen2 → 新 VM 接管),无需人工干预

### Requirement: 两代自举正确性

#### Scenario: Gen1 用旧 VM 编当前源
- **WHEN** 两代自举 Gen1
- **THEN** 用 SDK `bin/z42vm`(旧)跑旧 z42c → 产出 gen1 z42c + gen1 stdlib(旧格式外壳);
  旧 VM 能加载 gen1 产物

#### Scenario: Gen2 产出新格式
- **WHEN** 两代自举 Gen2(旧 VM 跑 gen1 z42c)
- **THEN** 产出 gen2 z42c + gen2 stdlib,其 zpkg minor == 当前源码 writer minor(新格式)

#### Scenario: 新 VM 接管
- **WHEN** cargo 新 VM(新 minor)加载 gen2 产物
- **THEN** 成功(版本一致),后续 build xtask / test / package 正常;`xtask.zpkg` 产出

### Requirement: 闭环自愈

#### Scenario: bump 后 publish-nightly 得以发布
- **WHEN** 格式 bump 的 push 触发 CI,两代自举使 build-and-test/host-package/package-* 全绿
- **THEN** publish-nightly 运行,发布携带**新格式**种子的 nightly → 下次 push 种子已是新格式 →
  走快路径 → 死结不再复现(免手动传种子)

#### Scenario: 旧 VM 缺失兜底
- **WHEN** 下载的 nightly SDK 缺 `bin/z42vm`(异常/旧包)
- **THEN** 明确报错(指明需带 bin/z42vm 的 nightly),不静默走单 VM 撞 strict-pin

## IR Mapping
无。纯 CI/toolchain 编排变更,不碰 z42c/VM/格式代码。strict-pin 不动。

## Pipeline Steps
- [ ] Lexer/Parser/TypeChecker/Codegen/VM:全不变
- [ ] ci-bootstrap action:版本差 gate + 两代分支
- [ ] (可选)xtask bootstrap-twogen 子命令
