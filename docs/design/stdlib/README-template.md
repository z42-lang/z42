# stdlib `<package>` README 模板

> 复制此模板到新 stdlib package 的 `src/libraries/<pkg>/README.md`，按内联
> 注释填空。**所有 stdlib package 必须有 README**；CI 后续会加 lint 检查。
>
> 质量标准（per docs/review.md Part 3 S2.8）：
> - **中等及以上**：≥ 40 行；含职责 / 核心文件 / 入口点 / 依赖 / Deferred 五段
> - **极简（< 30 行）** 只允许：单文件 + 单职责的包（如 z42.math 早期形态）
>
> 在归档 add-z42-<name> spec 之前必须达标。

---

```markdown
# z42.<name> — <一句话能力>

## 职责

<2-4 句：本包做什么；不做什么（边界）；纯脚本 / native 桥接 / 混合实现。>

例：z42.<name> 是 z42 标准库的 <主题> 子模块。**纯脚本实现** / **libffi 桥接**
/ **混合（X 用脚本，Y 走 native）**。本包**不**做 <边界外的事>（见 [`docs/design/stdlib/<name>.md`](../../docs/design/stdlib/<name>.md)）。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Foo.z42` | `class Foo` | <一句话职责> |
| `Bar.z42` | `static class Bar` | <一句话职责> |
| ... | ... | ... |

> 子目录（如有）：`<subdir>/` —— <subdir 用途>

## 入口点

### 公共类型 / 静态函数

- `Std.<Namespace>.Foo` — 工厂：`Create()` / `FromXxx()`；方法：`Do(...)` / `Reset()`；访问器：`Count()` / `IsEmpty()`
- `Std.<Namespace>.Bar.Helper(args)` — <作用>

### 常量

- `Bar.MAX_X` (...)
- `Bar.DEFAULT_Y` (...)

> 注：z42 当前不支持命名属性 getter（`Name { get { ... } }`），所有访问器均为方法。

## 依赖关系

依赖 `z42.core` <+ 其他>。`<其他>` 用于 `<具体场景>`。

无 native 依赖 / 通过 `__<prefix>_*` builtin 走 VM 内 `<lib>` 桥接。

## Deferred / Future Work

详见 [`docs/design/stdlib/<name>.md`](../../docs/design/stdlib/<name>.md) "Deferred / Future Work" 段。当前 v0
覆盖 <主题> 的 <子集>；未引入的内容：

### <Feature A>

- **来源**：<spec / 用户反馈 / roadmap>
- **触发原因**：<为何 v0 不做>
- **前置依赖**：<解锁需要什么>

### <Feature B>

- ...

## 测试

`tests/`：<N> 个 `.z42` 测试文件覆盖 <主题 / 边界 / 错误>。运行：

```bash
./xtask test lib        # 完整 stdlib 测试套
```

<可选：跨 zpkg 端到端 / golden / benchmark 等。>
```

---

## 模板检查清单

提交 spec 归档前对照：

- [ ] 一句话能力（首行 `# z42.<name> — <能力>`）
- [ ] 职责段：包含范围 + 排除边界
- [ ] src/ 核心文件表（每个 .z42 一行）
- [ ] 入口点：列出所有 `public` 类型和静态方法（这是事实上的 public API surface）
- [ ] 依赖关系：明确 stdlib 依赖 + native 依赖
- [ ] Deferred / Future Work：至少列 1-2 个明确"v0 不做"的项目，对齐
      [`docs/design/stdlib/<name>.md`](../../docs/design/stdlib/<name>.md) Deferred 段
- [ ] 测试段：tests/ 目录的测试文件数 + 运行命令
- [ ] 总行数 ≥ 40

## 反例（避免）

❌ 11 行 README + 单表无 API surface
❌ "TODO: write docs later" placeholder
❌ Deferred 段缺失，导致 `docs/roadmap.md` Deferred Backlog Index 没有
   索引行可指（违反 [`philosophy.md` 延后管理规则](../../agent/rules/philosophy.md#延后特性管理必须遵守)）
❌ src/ 文件表只列文件名不写类型 / 职责
❌ 依赖关系只写 "z42.core" 不写 "为什么"
