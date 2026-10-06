# cli-new-run-build-clean

学习手册第 3 章读者终端上会直接出现这些输出，所以进度行与 `removed` 行里的路径必须相对当前目录：
不打绝对路径（`{case}` 是用例目录的绝对路径，工程在它下面），更不能出现 `/./`。

`build` 一步只断言不变量：工程已被前面的 `z42 run` 建过，这次走增量，打的是
`no changes; preserved -> ./dist/hello.zpkg` 而不是 `cache -> …`。`-> ./` 同时覆盖 `wrote ->` / `preserved ->` 两种形态，
钉死某一行会随增量状态误报。
