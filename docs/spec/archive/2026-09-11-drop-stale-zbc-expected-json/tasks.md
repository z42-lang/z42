# Tasks: 删除 zbc-format 的死 `expected.json`（补完 #424 的清理）

> 状态：🟢 已完成 | 完成：2026-09-11
> 类型：`fix`（最小化模式）。

**变更说明：** 删除 `src/tests/zbc-format/*/expected.json`（6 份），并修掉两处指向它们的陈旧文档。
**原因：** 见下「事实」。
**文档影响：** `src/tests/zbc-format/README.md`、`.claude/rules/version-bumping.md`。

## 事实（删之前核过的）

1. **零代码消费者。** 全仓（含 CI yml、任意扩展名，排除 `.git`/`target`/`artifacts`）grep
   `expected.json` / `expected_json` / `expectedJson`：命中的**全是文档与归档记录**，无一处代码读它。
   `zbc_compat.rs` / `format_fixture_versions.rs` 读的都是 `source.zbc` 字节，不碰 JSON。
2. **落后 18 个 minor。** 六份全部声称 `header.minor = 20`，而当前 zbc 格式是 **38**。
   它们最后一次被有意维护是 2026-07-11（`drop-tsig-expt`，zpkg 0.31 时代）。
3. **README 本来就不认它们。** `src/tests/zbc-format/README.md` 的「核心文件」表只列
   `source.z42` 与 `source.zbc`。
4. **🔑 已有决定性先例。** PR **#424**「刷新全部格式 golden 字节基线 + 加两道防腐门」
   （commit `aab7013b`）**已经删掉了 `zpkg-format/*/expected.json` 全部 4 份**，
   同时把 `zpkg-format/README.md` 的核心文件表改成只列源 / 配方 / 字节基线——
   **但漏掉了 zbc-format 这 6 份**（那次只重生了它们的 `source.zbc`）。
   本变更 = 补完 #424 的同一次清理，不是新的判断。
5. **它们在主动误导人。** `enforce-test-attr-placement` 变更中，`with-tidx/expected.json` 被当作
   可信事实读取过——一个 18 个 minor 之前的快照。留着一份没有门禁兜底、且没人更新的"期望值"，
   比没有更糟。

## 为什么不是"重生它们"

要让它们活起来需要一个 zbc→JSON dumper **加一道防腐门**（否则立刻重新腐坏——过去 18 个 minor
就是证明）。那是一个**新功能**，与本次「清掉死物」不是同一件事；且 #424 已经为同类文件选了删除。
若将来确实需要「格式 bump 时可读的 diff」，应作为独立变更**连同门禁一起**做（形态参考
`format_fixture_versions` 那道门）。

## 任务

- [x] 1.1 删除 `src/tests/zbc-format/{cross-import-token,empty,multi-method,strp-func-minimal,with-frcs,with-tidx}/expected.json`
- [x] 1.2 `src/tests/zbc-format/README.md`：核心文件表加一行说明（对齐 zpkg-format README 的形态）
- [x] 1.3 `.claude/rules/version-bumping.md:168`：注释 `# 临时工程：name 匹配 expected.json（…）`
      指向的是 **#424 已删除**的 zpkg 版 expected.json → 改成按 fixture 目录名说明
- [x] 1.4 验证：`cargo test --test zbc_compat --test format_fixture_versions` 仍绿
      （证明确实没有消费者）


## 验证

- `cargo test --test zbc_compat --test format_fixture_versions`：**5/5 绿**——删掉 6 份 JSON 后
  两道字节防腐门与解码测试全部照常通过，反证「零消费者」。
- `xtask test`：**全 stage 绿**（含自举不动点 gen1==gen2）。
