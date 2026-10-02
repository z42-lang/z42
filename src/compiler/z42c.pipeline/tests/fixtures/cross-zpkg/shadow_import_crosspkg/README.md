# shadow_import_crosspkg — 本包遮蔽导入包同 FQN（E0606）

`main`（`demo.shadowapp`）自己声明了一个 `Demo.ShadowNs.Widget`，而它的依赖
`target`（`demo.shadowlib`）也声明了同一个 FQN。

**修前**：编译 rc=0、零诊断，本地那份赢——`bare` 与 `qualified` 都绑到本地，
依赖包那份**没有任何写法能指到**（两者 FQN 逐字相同，限定名也分不开）。想调只有它才有的
`OnlyLib()` 时，得到的是 `no method 'OnlyLib' on 'Widget'`——答非所问。

**为什么是 error 而不是 warning**：「本地恒赢」确实是既定规则，但规则明确 ≠ 结果可接受。
与 E0601（两个第三方依赖打架、下游无权修、故未引用则不报）不同，本条冲突的两份里
**有一份是本包自己写的**，改名随时可以 ⇒ 报 error 是可行动的。

负例 fixture 约定（`expected_build_error.txt`）见 `../README.md`。
