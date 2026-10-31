# sys — PAL 操作系统后端

本目录负责平台选择和 OS 实现；上层 concern 只提供平台无关接口和共享策略。

| 目录 | 职责 |
|---|---|
| unix/ | POSIX 实现，包括 Apple、Linux/Android 的必要差异 |
| windows/ | Windows 进程 job、线程栈、socket 与文件能力 |
| wasm/ | 保持无进程环境、无时钟和无熵源的既有降级行为 |
| fallback/ | 其他目标的原有降级实现 |
| common/ | 后端共享的宿主操作或空操作，不持有 VM 状态 |

入口是 mod.rs 的编译期选择。各后端实现相同的 concern 接口；涉及共享类型时使用 PAL 的类型。
生产代码的平台 cfg 与 libc 只允许出现在本目录，由 `xtask check layers` 及其构造自测守住。

行为测试通过 PAL 上层接口执行：`cargo test --lib pal::`（在 src/runtime 下），完整验证使用 `xtask test`。
机制见 [PAL](../../../../../docs/internals/src/runtime/pal.md)。
