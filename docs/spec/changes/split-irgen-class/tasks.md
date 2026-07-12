# Tasks: 拆 IrGen God-Class（review P1-3，refactor）

> 状态：🟡 进行中 | 创建：2026-07-12 | 子系统：compiler(semantics)
> 同 P1-1/2 手法：IrGen 作 mediator（持字符串池 + _symbols + 低层助手 Intern/_q），
> 子构建器持 _g 反向引用。

## 进度概览（每步独立不动点 + 单独 commit）
- [x] 1. 抽 `TestIndexBuilder`（[Test] 发现 + TIDX 构建 15 方法）—— ✅ 不动点 7/7 + golden 138/138 + TIDX 8 passed（IrGen 1419→1188）
- [x] 2. 抽 `ClassDescBuilder`（类/接口/属性描述符 + attr 11 方法）—— ✅ 不动点 7/7 + golden 138/138（IrGen 1188→890）
- [ ] 3. 抽 `StubEmitter`（native/abstract/delegate/autoprop 桩）
- [ ] 4. 拆巨函数 Generate（~317 行）→ 子函数，IrGen <500

## 备注
- 脚本：find_method 声明锚定 + net_braces 跳字符串/注释花括号（P1-2 踩坑固化）。
- _ti* 状态随组移入 TestIndexBuilder（public，Generate 经 _testIx 读产出）。
