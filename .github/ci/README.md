# CI 控制脚本与回归

## 职责

处理 GitHub 事件、测试差异基线、runner 恢复与发布上传等基础设施。产品构建、用例发现和测试执行由 xtask 负责；
这里不维护逐用例的 CI 名单或平台配置。

## 功能索引

| 入口 | 职责 |
|---|---|
| `changed-base.sh` | PR 使用 `XTASK_PR_BASE`，push 使用 `XTASK_PUSH_BEFORE`；验证 Git commit 后只输出基线 SHA，空输出表示全量 |
| `xtask` | 用仓库 SDK 的 VM / libs 执行正式 xtask.zpkg |
| `main-supersede.sh` | 按 nightly 到提交的累计 SDK 差异保留发布者、替换无 SDK 改动的旧运行 |
| `retry_runner.py` | 有界恢复未取得 runner 且未执行步骤的 CI 故障 |
| `upload_release_assets.py` | 上传后核对远端资产，处理瞬时上传故障 |
| `test_*.py` | 对应基础设施的事件、错误传播与状态转换回归 |

## 如何测试验证

```bash
python3 -B -m unittest discover -s .github/ci -p 'test_*.py'
```

`test_changed_base.py` 使用临时本地 Git 仓库与 origin，检查真实 fetch / diff，并执行 workflow 的实际 Bash 测试步骤。
SDK、编译器和 Runtime 不是这些基线选择测试的前置。新控制行为的用例追加到对应 `test_*.py`，确认被 discover 执行。
`test_main_supersede.py` 用 GitHub 文件列表夹具执行 nightly 差异判断和 workflow 的纯文档检查，
覆盖重命名跨越生产、私有测试及文档边界。
`test_package_selection.py` 核对实际 workflow 的打包与发布条件，覆盖 PR、main push、定期和手动事件。
改动事件或调用点后还要跟进 GitHub CI；`xtask check ci-shell` 检查 composite action 的 shell 变量约束。

## 关联文档

- [CI 拓扑与选择边界](../../docs/internals/src/devinfra/ci.md)
- [用例归属与新增步骤](../../docs/internals/src/devinfra/test-layout.md#1-用例放哪)
