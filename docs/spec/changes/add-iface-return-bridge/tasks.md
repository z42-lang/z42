# Tasks: 接口返回位的 struct 协变（形状 A′）

> 状态：🚧 进行中 | 创建：2026-09-25 | 类型：lang（需规范先行）
> 规范：[proposal.md](proposal.md) · [design.md](design.md)（D1 = 形状 A，按 A′ 实现，User 2026-09-25 裁定）

## 为什么分阶段（不是「两条一起做」的退让）

User 裁「两条一起做」= **不要把「只报错」当最终答案**；本 tasks 仍在同一个 change 里交付
止血 + 可用两件事，但拆成可独立验证的阶段，理由是硬约束而非偏好：

- **阶段 3（跨包）与阶段 2（本包）之间可能必须跨一个 nightly**：桥接要进 TSIG / 导出元数据，
  若给 `z42.ir` 的模型加字段并被 `semantics` 引用 = 新跨成员符号 ⇒ `bootstrap-seed.md`
  的 support/use 纪律（#788/#789 同形）。**能否骑既有通道要到阶段 2 末才确定。**
- 阶段 1 单独就能消灭「编译期零诊断 + 运行期崩」——先落地它，等于任何时刻中断都不留静默坑。
  ⚠️ **更正**（本文件初版把触发面写窄了）：阶段 1 的诊断必须**也覆盖主形状**（接口声明返回引用型），
  否则阶段 1 落地后那条崩溃仍在。阶段 2 用桥接把主形状变成可跑，届时**收窄**该诊断到
  「桥接无法成立」的形状（design D2）。这一小段「先报错、后放行」是刻意的安全顺序，不是返工。

## 阶段 1：止血 —— ABI 不自洽的协变不再静默

- [ ] 1.1 `InheritanceResolver` 协变判定（`:481-484`）后加 ABI 兼容判据：`implRet` 是 blob struct
      而 `wantRet` 不是 ⇒ 记下「需桥接」（阶段 2 消费）；**桥接无法成立的形状**（见 design D2：
      接口声明返回另一个 blob struct / 基元）⇒ 报新诊断
- [ ] 1.2 分配新诊断码：扫 main **+ 每个在飞 PR 分支**的 `DiagnosticCodes.z42`；发射点先用字面量、
      进 `scripts/test/diag-literal-emitters.txt` 挂账（当日日期）
- [ ] 1.3 单测：proposal 的 10 行最小复现 → 阴性；**一字段 struct 返回 → 阳性仍通过**
      （判别力成对，否则拦的是「struct 返回」而非「ABI 不自洽」）
- [ ] 1.4 `error-codes.md` 登记（规则 ⑤ 双向相等）
- [ ] 1.5 GREEN + fingerprint 判定（**修前那种写法编得过 ⇒ 要 bump**，与 #795/#796 同理）

## 阶段 2：桥接合成（本包内）

- [ ] 2.1 具体实现挪到合成名 `<m>$struct`（沿用既有 `$` mangle 惯例；**先确认它不与 arity
      mangle `name$N` 撞命名空间** —— `$ctor`/`$indexer`/`$cctor` 是同族先例，`SurfaceHash` 那套
      `$` 名**不是**同一命名空间，切勿合表）
- [ ] 2.2 合成桥接占**裸名**：签名取接口声明（返回 `R_i`、无 sret），体 = 调 `<m>$struct`
      → `__box_struct` → return。合成落点参照 `RecordSynth` 的先例
- [ ] 2.3 直接调用点静态绑到 `<m>$struct`（`MemberResolver`）；**接口调用点一字不改**
      （裸名槽已是桥接）
- [ ] 2.4 跟按名字判断的那几处：`ChainHasMethod` / devirt（`ResolveSealedTarget`）/
      `ReceiverMethodIsVirtual` —— 漏一处就是静默走错方法
- [ ] 2.5 **虚覆盖一致性**：该方法若在类层次里被覆盖，子类覆盖也必须是桥接形态，否则子类槽变回
      sret ⇒ 同一个崩溃换个入口回来。需要在继承解析处强制
- [ ] 2.6 e2e：用户自定义接口 + 两字段 struct 返回，经接口调用真跑通；**退回对照**验判别力

## 阶段 3：跨包

- [ ] 3.1 桥接与 `$struct` 都要进导出元数据；导入侧解析出同一形状（**不在导入侧重新合成**）
- [ ] 3.2 判定是否需要格式 bump / support-use 两阶段（见上「为什么分阶段」）
- [ ] 3.3 跨包 e2e（`src/tests/cross-zpkg/`）

## 阶段 4：use —— 解锁 `List<T> : IEnumerable<T>`

- [ ] 4.1 `ListEnumerator<T> : IEnumerator<T>`（`Current` 属性 ↔ 接口 `get_Current` 访问器形态要对齐）
- [ ] 4.2 `List<T> : IEnumerable<T>`
- [ ] 4.3 e2e：经 `IEnumerable<int>` 静态类型 foreach / 传参 / 赋值三条；
      **不回归**：`List<T>` 直接 foreach 仍走索引快路径（无装箱前提不变）
- [ ] 4.4 `Dictionary` 同款（`DictionaryEnumerator` 已存在）—— 视阶段 4.3 结果决定是否同期做
- [ ] 4.5 文档：`tuples.md` 无关；要动 `docs/reference/src/stdlib/collections*.md` 与
      `Protocols/IEnumerable.z42:8` 那句「目标」注释（它终于成真）

## 不做（本 change 之外）

- 显式接口实现语法（`IFace.Member`）
- prelude 的 `GetEnumerator` 返回裸 `IEnumerator`（design D3 末：裸名可能**恰好**让协变判定通过，
  动它之前要先确认方向）⇒ 独立立项
- LINQ / 集合视图
