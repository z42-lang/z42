# class_internal_access — 跨包 internal 类引用强制（自动门）

## 自动门

cross-zpkg runner 的 `expected_build_error.txt` 约定：`main` 必须**编译失败**且 stderr 含该文件内容；
编过了判红，错误文本对不上也判红。本 fixture 据此纳入 `./xtask test e2e --dir cross-zpkg`。
（不放 `expected_output.txt`：期望构建失败，无 stdout 可比对。）

## 期望行为

- `target/`（`demo.aclinttarget`）：`Secret`（无修饰符 → 默认 internal）、`SecretExplicit`（显式 internal）、
  `Api`（public）。
- `main/`（`demo.aclintapp`，依赖 target）：
  - `new Api()` → 放行（跨包 public 类）。
  - `new Secret()` / `new SecretExplicit()` → `E0404 AccessViolation`
    `cannot access internal class \`Secret\` from another package`。
- E0404 为非阻断诊断，但错误计数非零 → `z42c build main` 退出非零。

## 期望的失败文本

`expected_build_error.txt`：``E0404: cannot access internal 类 `Secret` from another package``
（同一次构建里 `SecretExplicit` 那条也会报；门只需命中其一即可锁住行为）。
