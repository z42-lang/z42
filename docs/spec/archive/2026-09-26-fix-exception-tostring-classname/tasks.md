# Tasks: 异常 `ToString()` 按运行期类型取名

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26 | 归档：2026-09-26
> 分支/worktree：`learn-ch20` @ `wt-learn19` | 基于：origin/main `6cb5528e8`（#861）
> 类型：`fix`（stdlib 一行；**无格式 bump、无新诊断码、编译器与 VM 一行不改**）
> User 已裁决：修基类，**不动那 17 个冗余 override**

**变更说明：** `Exception.ToString()` 从硬编码字面量 `"Exception: "` 改为
`this.GetType().Name + ": " + this.Message`。

**原因：** 用户自定义的异常子类一律打错类名（`class NotFoundException : Exception` 的实例
`ToString()` 得到 `"Exception: …"`）。17 个 stdlib 子类看起来没事，只是因为**每一个都重复
硬编码了自己的名字**。⭐ 这条在
`archive/2026-04-25-add-core-exception-ienumerable/design.md:247` 的风险表里登记过
（「Exception 类名硬编码 ToString 误差 | 低 | 9 个子类都显式写」），但那条缓解**只覆盖
stdlib**、没考虑用户自定义异常；「L3-R 后可统一」说的是清理冗余 override，不是这个
用户可见缺陷 ⇒ **用户侧是被漏掉的，不是被延后的**。

**文档影响：** `docs/reference/src/language/exceptions.md`（ToString 行为 + 📜 历史注记 +
两处 JIT trace 过时订正 + 「当前限制」补裸 `throw;`）、`Exception.z42` 抬头注释、
学习手册第 20 章（另一个 commit）、第 11 章一句谎报订正。

- [x] 1.1 `Exception.z42`：`ToString()` 改用 `this.GetType().Name`
- [x] 1.2 验修法可行：基类方法里 `this.GetType()` 在派生实例上取到真实类名
      （三种形态实测：直接 new 派生 / 基类自身 / 经基类静态类型调用）
- [x] 1.3 🔴 **阴性对照：stdlib 子类输出必须一字不变** —— 逐条实测
      （`ArgumentNullException` / `InvalidOperationException` / `KeyNotFoundException` /
      `NotImplementedException` / `OverflowException` / `InvalidCastException`）+ `Exception` 自身
- [x] 1.4 文档同步（按 doc-system 三问，见下）

## 验证

- `xtask test all` → **✅ GREEN — all stages passed**（15/15）
- e2e goldens 单独跑过一轮：**734 + 84 + 3 passed / 0 failed**
- `test docs` / `test lines` 在文档与注释改完后**复跑**绿（`0 new/grown`）
- ⭐ **「零命中」的解释**：全仓**没有任何 golden 打印用户自定义异常的 `ToString()`** ——
  stdlib 子类各自硬编码恰好掩盖了缺陷。学习手册第 20 章 `examples/types/exceptions/custom/`
  是**首份覆盖**（`NotFoundException: 找不到：bob` 被钉进 transcript）。

## 阶段 9 文档同步（doc-system 三问）

1. **用户能看见吗？** ✅ `reference/language/exceptions.md`（ToString 行为 + 📜 注记；
   顺带两处过时 + 一处缺失，见下）；学习手册第 20 章 + `examples/types/exceptions/`
2. **下一个接手的人不读文档能看懂吗？** ✅ 修法理由与「为什么 17 个 override 变冗余」
   写在 `Exception.z42` 就地注释；`internals` 无对应机制页需改（这不是新机制，是修一行）
3. **目录结构 / 入口 / 依赖变了吗？** ❌ 未增删文件

**正交三处**：根 README ❌；`docs/roadmap.md` ❌（未新增/消化延后项 —— 那 17 个冗余
override 的清理仍是原风险表里的「L3-R 后可统一」，本刀没做也没改变它）；
`docs/agent/rules/` ❌。

## 🆕 顺带订正的两处过时 + 一处缺失（同属本次文档同步）

- 🔴 **「JIT 路径不填 StackTrace」过时**（`exceptions.md` 正文 + 「当前限制」表 +
  `Exception.z42` 抬头注释，共三处）：`jit/helpers/control.rs` 的 throw helper 调的就是同一个
  `populate_stack_trace`，引入于 **`820e583ce` "feat(jit): stack trace parity with interp"
  （2026-05-10）** —— 与 interp 那半**同一天**落地，文档从未跟上。
  ⚠️ **验法已写进文档**：不能只看 `--mode jit` 跑通 —— 我第一个探针里抛出的 `Inner` 根本没被
  编译（`Z42_JIT_PROFILE=1` 只见 `Main` 的 `lazy-compile`）⇒ 抛出发生在解释帧里，证不了任何事。
  正解是把抛出函数**跑热**（20 万次）到出现 `lazy-compile <fn>` 再抛。
- 🔴 **「当前限制」表漏了裸 `throw;` 不支持**（解析错误）。已补 + 替代写法 `throw e;`
  （重抛不覆盖已填的 StackTrace ⇒ 等价且不丢栈）。
- 🔴 **第 11 章一句谎报**（学习手册）：「数组越界不是可 `catch` 的异常」「`try`/`catch`
  接不住它」—— 实测 **`catch { }` 能接住并继续执行**。该章 transcript 是准的（示例根本没写
  try/catch）⇒ **错的是解释、不是现象**。已改成说清机制 + 明确「正确做法仍是先查范围」。

## 🔴 流程自查

**我漏了最小化模式的必备件**：workflow §最小化模式明确「`docs/spec/changes/<name>/tasks.md`
← 必须有」，而我只改代码 + 文档就直接 commit 了，这份 tasks.md 是**事后补的**。
下次：fix 类也要先建 tasks.md 再动手。
（上一刀的自查是「归档顺序走反」，这次是「最小化模式的容器没建」—— 两次都是**流程件漏做**，
不是技术判断错。）
