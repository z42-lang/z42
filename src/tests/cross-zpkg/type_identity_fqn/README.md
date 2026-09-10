# type_identity_fqn — 跨包类型身份门

**这道门盯的是本仓库一条容易悄悄烂掉的不变量：持久化的类型身份必须是跨命名空间唯一的。**

两个依赖包各声明一个短名同为 `Widget` 的类。若 zpkg 里只写短名，消费端只能靠短名竞争猜——
实测（`unify-type-identity-fqn` 之前）`HolderA.w` 与 `HolderB.w` **双双退化成同一个无句柄合成
类型 `Widget`**，两者不可区分。本门断言它们各自绑到自己 ns 的那份，并顺带覆盖接口 / enum 的
FQ 身份与「基元**不**被 FQ 化」。

## 判别力已验证（不是空门）

把编译器退回 `526acb72`（本 change 之前）重建后跑本用例：**FAIL**；含本 change：**PASS**。
中间还抓到过一个更隐蔽的形态——只把发射端改成 FQN、尚未修「裸名解析不看引用方 ns」时，
`HolderA.w` 会被写成 **`Demo.FqnBeta.Widget`**（自信的错答案，比修前的降级更坏）。
详见 `docs/spec/changes/.../evidence/ambiguity-gate.md` 的三态对照表。
