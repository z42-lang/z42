# Tasks: 栈对象字段访问接 inline cache

**变更说明：** interp 的 `Value::StackObject` 的 FieldGet/FieldSet 复用堆路径同款单态
`FieldIC`（缓存 `TypeId→slot`），消除每次访问的 `field_index` 字符串哈希查找。
**原因：** 栈 FieldGet 原本"No PIC"、每访问一次哈希查一次；密集字段访问（对象传进 callee 反复读字段）
下比堆对象慢 → 使 escape/#118/跨过程摘要的栈分配在该模式下反被堆反超。IC 后栈字段访问≈堆字段访问。
**文档影响：** book escape-analysis-stack-alloc.md「运行时触达面/per-context arena」补一句 IC；
`add-escape-analysis-stack-alloc` 注释"No PIC (stack ... not mega-hot)"已被推翻，改。

- [x] 1.1 field_get StackObject 臂接 FieldIC（lookup 命中→直接 slot；miss→field_index+install）
- [x] 1.2 field_set StackObject 臂接 FieldIC（先算 slot 释放借用再写；无写屏障——栈非堆槽）
- [x] 1.3 量测（同 bytecode base-VM vs new-VM，Heavy 密集字段访问 8M）：interp **+5%(1.05×)**、jit 中性(1.00×，堆路径)、退出码一致
- [ ] 1.4 cargo test --lib（runtime 单测，xtask test 不含）
- [ ] 1.5 xtask test 完整 GREEN
- [x] 1.6 文档同步：book escape 页 IC 说明 + 改 "No PIC" 注释
