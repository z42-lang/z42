# interface_internal_access — 跨包 internal 接口引用强制（自动门）

> change：`complete-class-access-control`（④ 接口类型可见性）。

## 现在是自动门了（2026-09-13）

> 曾是「手工验证 fixture」：cross-zpkg runner 当时只支持**成功运行 + stdout 比对**
> （`expected_output.txt`），表达不了「期望构建失败」，于是本目录**故意不放** `expected_output.txt`
> ⇒ runner 直接跳过它 —— **连 FAIL 都不是，是根本没跑**。

`report-crosspkg-duplicate-type`（#576）给 runner 加了 `expected_build_error.txt` 约定：
`main` 必须**编译失败**且 stderr 含该文件内容；编过了判红，错误文本对不上也判红。
本 fixture 已据此转为自动门，纳入 `xtask test e2e --dir cross-zpkg`。

## 期望行为

- `target/`（`demo.ifaceinttarget`）：`Handler`（无修饰符 → 默认 internal）、`HandlerExplicit`
  （显式 internal）、`Api`（public）。
- `main/`（`demo.ifaceintapp`，依赖 target）：
  - `Use(Api a)` → ✅ 放行（跨包 public 接口）。
  - `UseInternal(Handler h)` / `UseInternalExplicit(HandlerExplicit e)` → ❌ `E0404 AccessViolation`
    `cannot access internal interface \`Handler\` from another package`。

## 期望的失败文本

`expected_build_error.txt`：``E0404: cannot access internal 接口 `Handler` from another package``
（同一次构建里 `SecretExplicit` / `HandlerExplicit` 那条也会报；门只需命中其一即可锁住行为）。
