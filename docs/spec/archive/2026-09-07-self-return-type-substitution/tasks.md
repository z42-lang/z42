# Tasks: `Self` 返回类型替换

> 状态：🟢 完成（已归档）| 创建：2026-09-07 | 归档：2026-09-07

## 进度

- [x] 1. 探针确认现状（接口收者 `:Self` 裸奔 / 具体类收者 `:Point` 正确）
- [x] 2. `MemberResolver._substSelf`（镜像 `_substGeneric` 的递归结构）
- [x] 3. 接通两个接口收者返回点（方法调用 `:96` + 属性 getter `:197`）
- [x] 4. 更新 2 条既有 `Self` 用例（它们断言的正是本次要改掉的旧行为）+ 新增 1 条具体类对照组
- [x] 5. **退回对照实测**（见下）
- [x] 6. 完整 GREEN（0 failed，3m03s）+ 不动点 3/3 gen1==gen2
- [x] 7. 文档同步（generic-constraints.md 改写「实现模型」条目 + roadmap.md Deferred 标 ✅）+ 归档

## 🔒 退回对照实测结果（关键证据）

把两个 `_substSelf(...)` 调用点退回成直接返回 `Signature.Ret`，重建后跑 `xtask test compiler`：

```
FAIL test_self_return_type_substituted_to_receiver_interface:
  expected (block (decl x :IClone (call-inst (ident c :IClone) Clone :IClone)))
  but got  (block (decl x :Self   (call-inst (ident c :IClone) Clone :Self)))
FAIL test_self_coexists_with_interface_own_type_param:
  expected …:IBox…  but got …:Self…
PASS test_self_on_concrete_receiver_still_gives_concrete_type      ← 两态同绿
```

⇒ 两条断言是**真门**；第三条是**控制组**（具体类路径不经 `_substSelf`），用例注释已写明它不是门，
避免将来有人误以为它在守什么。这遵循本仓既定纪律：新用例先做退回对照再当门用
（历史教训见 `docs/spec/archive/2026-09-06-add-associated-types/` 的空测试故事）。

## 边界

- **只动接口收者的返回位**。形参位的 `Self`（`bool Same(Self other)` 经接口调用）不在本轮，
  理由见 proposal「不在本轮」：那是逆变方向，没有唯一安全上界。
- **不碰具体类收者**：它走实现方签名，本来就正确。
- **不引入新错误码**：本变更是类型推导改进，不产生诊断。
