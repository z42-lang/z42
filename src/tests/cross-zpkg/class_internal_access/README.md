# class_internal_access — 跨包 internal 类引用强制（手工验证 fixture）

> change：`enforce-crosspkg-internal-class`（类级访问强制 ②）。

## 为什么是「手工验证」而非自动 case

cross-zpkg 自动 runner（`scripts/test/xtask_test_cross.z42`）只支持**成功运行 + stdout 比对**
（`expected_output.txt`）——**无 expected-compile-error 模式**。本 fixture 期望 `main` 构建时报
`E0404`（跨包引用 internal 类），属**构建失败**语义，无法用 stdout 比对表达。故本目录**故意不含
`expected_output.txt`**，runner 会跳过（`if (!File.Exists(dir/expected_output.txt)) continue`），
不影响 GREEN。与成员级 `enforce-access-control`（#180）跨包 internal「手工验证覆盖」同款处置。

## 期望行为

- `target/`（`demo.aclinttarget`）：`Secret`（无修饰符 → 默认 internal）、`SecretExplicit`（显式 internal）、
  `Api`（public）。
- `main/`（`demo.aclintapp`，依赖 target）：
  - `new Api()` → ✅ 放行（跨包 public 类）。
  - `new Secret()` / `new SecretExplicit()` → ❌ `E0404 AccessViolation`
    `cannot access internal class \`Secret\` from another package`。
- E0404 为非阻断诊断，但错误计数非零 → `z42c build main` 退出非零。

## 手工验证步骤（需 0.38 全栈：CI 两代自举，或某个 0.38 nightly warm 本地）

```bash
# 1) 建 target lib（产出 demo.aclinttarget.zpkg）
Z42_LIBS="$PWD/.z42/libs" .z42/bin/z42c build src/tests/cross-zpkg/class_internal_access/target/z42.toml --release
# 2) 把 target.zpkg 放进 main 的 libs 后建 main —— 期望非零退出 + 打印两条 E0404（Secret / SecretExplicit）
#    Api 无诊断。
Z42_LIBS="<stdlib + target dist>" .z42/bin/z42c build src/tests/cross-zpkg/class_internal_access/main/z42.toml --release
```

> 本地 macOS 因格式-bump 两代自举墙无法产 0.38 stdlib（见 memory `escape-stack-format-bump-ci-learnings`）；
> 权威验证在 CI（0.38 全栈）或等 0.38 nightly 发布后本地 warm。
