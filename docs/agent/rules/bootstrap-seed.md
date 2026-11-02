# 自举种子与分阶段引入

> 触发条件：改「构建工具自己也要被构建」的链路（z42c、xtask、stdlib 的构建 / 供种 / 自举路径）；或让 z42c、
> stdlib、xtask 的源码开始使用新语法、新 API、新清单键；wire 格式变化另见 [version-bumping.md](version-bumping.md)。
>
> 为什么需要种子、种子从哪来、四个约束轴与破环、CI 怎么冷启动，见书：
> [编译器自举](../../internals/src/compiler/self-hosting.md) ·
> [xtask · 种子从哪来](../../internals/src/devinfra/xtask.md) ·
> [CI · 自举种子从哪来](../../internals/src/devinfra/ci.md)。本文件只放必须遵守的纪律。

---

## 一、删种子兜底：与替代来源同一个变更（必须遵守）

z42c、xtask、stdlib 互为前置，任何「从源码全新构建」的入口都必须先有一个已存在的种子。所以：

1. **删除任何构建期种子 / 兜底路径，必须和「为所有入口提供替代种子」在同一个 PR 里落地**——不拆成两步，
   不「先删兜底，回头再补」。
2. **「所有入口」里冷启动入口最容易漏**：本地全新 checkout，以及 CI 上每条在全新 runner 上冷建的 job
   （构建、打包、只下载种子就跑的 gate）。逐条核对 [CI 拓扑](../../internals/src/devinfra/ci.md)的 job 表。
3. **本地验证可复现的冷路径，CI 验证全新 runner 的入口**：两者不能互相替代；push 后跟进 CI，失败时修复或回滚。
4. **不要和格式 bump 踩在同一周期**：格式 bump 当次本来就有 job 会红一跑，两件事叠在一起分不清是谁的锅。

## 二、support 先行，晚一个 nightly 再 use（铁律）

**当前源码须能被已发布种子编译。** 源码消费种子尚不支持的语法 / 清单键 / API 时，先发布支持，再切换使用；
第二步等待 nightly **包含**支持提交。自依赖 API 的例外见下文；wire 格式 bump 按 version-bumping 与 CI 自举机制处理。

| 引入什么 | 第一步：support | 第二步：use |
|---|---|---|
| 新语法 | z42c 支持它，源码仍用种子可编译的写法 | 源码开始使用 |
| 新清单键或键改名（`z42.toml` / `z42.workspace.toml`） | 解析器认识新键（改名时新旧并存） | 仓内清单写新键 / 删旧名 |
| xtask、z42c 源用到的 stdlib 或编译器域 API | 加新 API，删改时旧 API 暂留供种子启动 | 切全部调用点，同一提交删旧 API |

- **「nightly 已包含」按提交判断，不按时间**；怎么确认见 [CI 拓扑](../../internals/src/devinfra/ci.md)。
- **z42c 运行期自依赖的那几个库**有例外：加 API 可以同一提交就用；改名、删除、给导出类型加字段仍有约束，
  见[编译器自举 · 轴④ 与改名](../../internals/src/compiler/self-hosting.md)。stdlib 源本身不受 API 这一维约束。
- 第一步的 PR 任务中记录消费侧的前提与收尾动作；确认 nightly 已包含支持提交后再切换调用点，
  同步删除已消失的过渡形态说明。不按日历天数判断种子是否可用。
- 种子所需的新旧键 / API 短期共存是[兼容规则的种子例外](philosophy.md#不为旧版本提供兼容)；
  必须记录移除条件，消费侧切换后删除，不扩展为长期兼容层。

## 三、边界检查（必须跑）

改了 z42c 的 lexer / parser / codegen / 格式 writer、加了新语法、bump 了格式，或新增 / 收敛了 z42c 运行期自依赖的库之后，
跑自举边界检查（命令与判读见[怎么跑测试 · 自举边界](../../internals/src/devinfra/testing.md)）：

- 上一个 nightly 编得过当前 z42c 源 → 没越界；
- nightly 编不过、当前 z42c 编得过 → **越界**：按上面拆成两步，或回退过早的使用。
- 运行期自依赖的问题本地 warm 验不出来，**新增 / 收敛这类库时，冷启动路径本地必验**（配方见
  [编译器自举](../../internals/src/compiler/self-hosting.md)），别只验 warm 就合。
