# Tasks: unify-type-identity-fqn

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：fix（compiler，改类型解析语义 + 持久化类型名）

**变更说明：** zpkg 的 TYPE / SIGS 里持久化的**类型身份**从**短名**改为**全限定名**。
短名不是跨命名空间的唯一键 —— 消费端只能靠短名竞争猜。出身是
[restore-emit-zbc-diagnostics](../../archive/2026-09-10-restore-emit-zbc-diagnostics/) 欠债表的
最后一项 **B3-产出端**（限定名被写成字面量 `"unknown"`），追根因后升级为独立 change。

## 落地（8 提交）

| # | 提交 | 内容 |
|---|------|------|
| 1 | class 切片 | 删「限定名 → `"unknown"`」短路；导入类补 `Namespace`；两个发射端改发 `Fqn()` |
| 2 | **根缺陷** | 裸名类型引用按**外围命名空间**解析（`WithCu`/`ScopeNs`）；接口 + 构造泛型 FQ；消费端 FQ 解析 |
| 3 | 接口 / enum | 补 `InterfacesByFqn` / `EnumTypeNs`；修 `zbc.md` 腐坏的 TYPE 布局 |
| 4 | 门 + 回归 | 跨包类型身份门；修两个自引入回归 |
| 5 | 断言 | 更新编码了旧行为的签名断言；DRAFT 同步实现实录 |
| 6 | 歧义守卫 | `ScopeUsings` + `IsBareNameAmbiguous`；歧义时**退回短名**，绝不写「选了赢家」的 FQN |
| 7 | 文档 | 补录上次 bump 漏写的 changelog；记录不做格式 bump 的裁决 |
| 8 | 跨包限定名 | first-wins 守卫收窄到只管裸名表；`_mergeImports` 并入 FQN 视图 |

## 修掉的真 bug

1. **B3**：限定名 → `"unknown"` ⇒ z42.core 四个反射类 **37 字段中 9 个**读回 `unknown`（实测归零）。
2. **裸名不看引用方 ns**：`Classes` 按裸名键 first/last-wins ⇒ `namespace Alpha` 里的 `Widget`
   绑到 `Beta.Widget`。`fix-type-ref-ns-collision` 当年**只修了限定引用那半边**。
3. **跨包限定名解析**：建型整个罩在裸名 first-wins 守卫内 ⇒ 同短名的第二份**压根不建型**。
   ⭐ 此前修不了 —— 修它要算导入类的 FQN，而导入类 `Namespace` 恒为空，本 change 才补上。

## 验证

- 完整 GREEN 全绿（`REAL_EXIT=0`）+ **自举不动点 3/3 gen1==gen2**，rebase 到 `a36566bf` 后重跑仍全绿。
- 运行期探针：反射面 `unknown` **9 → 0**，全差分恰好 9 行零回归（`evidence/probe-{before,after}.txt`）。
- 跨包门 `src/tests/cross-zpkg/type_identity_fqn/`：**判别力已验证**——编译器退回 `526acb72`
  重建后 FAIL、含本 change PASS；三态对照表见 `evidence/ambiguity-gate.md`。

## 不做 / Deferred

- ❌ **格式 bump**（User 裁决，proposal §9）：实测双向互操作都正确，且带 bump 无法本地全绿
  （两代自举是 CI 的活）。代价=新旧产物可静默混用，已明写。
- 🕳 **A3：E0456 补到声明位**（字段/形参/返回类型）—— 判据在 TypeChecker（有 usings），
  声明位检查在 collector 阶段（拿不到 usings）+ collector 诊断可见性另有历史包袱 → 独立立项。
- 🕳 统一 SIGS 与 TYPE 的**实参拼写**差异（裁决 D2 明确排除）。

## 顺带修的文档腐坏（5 处）

`zbc.md` TYPE 段布局（写作 `[1] type_tag, [2] type_param`，实际是 `u8 tag + u32 type_str_idx`）；
`zbc.md` changelog 缺 1.38；`zpkg.md` 缺 0.43；`zpkg.md`「当前版本」停在 42；
`Z42Type.z42` 声称「导入侧会设 Namespace」的假注释。
⭐ 全是同一形状：**没有东西盯着的断言迟早变谎言**。
