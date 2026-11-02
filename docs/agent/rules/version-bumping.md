# 格式 bump 与编译器指纹纪律

> 触发条件：改了 `.zbc` / `.zpkg` 的 wire 格式；或改了编译器的输出（codegen / 优化 / 类型检查 / lowering /
> 发出的诊断）但没有 bump 格式。
>
> 版本常量在哪、当前是几、改哪些文件、本地怎么重建与重生 fixture，全部见
> [格式 bump：清单与本地验证](../../internals/src/devinfra/format-bump.md)（唯一 SoT）。本文件只放必须遵守的纪律。

## 格式 bump（必须遵守）

1. **writer 与 reader 的版本号同一个提交一起改**。pre-1.0 是 strict-pin：版本不匹配直接拒读，没有兼容回退。
2. **改了 wire 格式就必须 bump minor**，同一提交重生入库字节基线，并更新两端的格式语义与约束说明。
   旧产物随之全部失效是预期行为，不写任何兼容代码（见 [philosophy.md 不为旧版本提供兼容](philosophy.md#不为旧版本提供兼容)）。
3. **只修 reader / writer 的非格式 bug（不改 wire 布局）→ 不 bump**。
4. **按清单完成专门自检**：可本地执行的提交前跑；仅由 CI 覆盖的检查跟进结果，不把完整 `xtask test` 当作全部格式验证。
5. 旧种子相关 CI 可能因版本失配失败；按 [CI 拓扑](../../internals/src/devinfra/ci.md)定位、重建或重跑，
   不把「下一次会自愈」当作验证结论。合并条件统一按 workflow 的门禁与必需 CI 检查。

## 编译器语义指纹（必须遵守）

同一份源码、同样的 wire 格式，编出的字节却变了——增量缓存靠指纹察觉这种变化。

| 情形 | 动作 |
|------|------|
| 改了编译器输出（含发出的诊断），**且没有 bump 格式** | 追加一条指纹条目 |
| bump 了格式 minor | 不必追加（格式变化已让旧缓存全部失效） |
| 只修 reader / writer 的非格式 bug，编出的字节与诊断都不变 | 不追加 |

指纹登记在哪、怎么追加、CI 怎么守门，见[编译器语义指纹](../../internals/src/compiler/compiler-fingerprint.md)。
