# field_null_mark_crosspkg_unchecked（负例：跨包 `?` 字段不检查就解引用）

`field_null_mark_crosspkg` 的**正面对照**。`main` 跨包读 `Holder.Marked`（标了 `?`）而不检查
⇒ 期望编不过，输出含 `is marked \`?\` and may be null here`（**E0484**）。

## 为什么必须有这一条

若只留正例（快照写法），
那条用例在「哨兵根本没过包边界」时**照样全绿** —— 不受检也能编过、输出一模一样。
⇒ 只有这条负例能分辨「机制接通了」与「机制恒不响」。
（**全量零命中既不能证明规则对、也不能证明 pass 接通**。）
