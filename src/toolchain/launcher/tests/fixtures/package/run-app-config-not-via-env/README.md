# run-app-config-not-via-env

app 侧车**不经 env 传递**。两条一起断言，少任何一条这个用例都守不住它该守的东西：

1. `Z42_APP_CONFIG` 不在 app 进程的环境里 —— env **会被子孙进程继承**：一个 z42 程序 spawn 出的每个子 app
   都会带着**父 app** 的侧车路径，且它是相对路径，子进程换 cwd 就指向不存在的文件 ⇒ 子 app 自己的运行时设置
   被整份丢掉。
2. `[profile.*.runtime]` 仍然到达了 app（`MODE_SRC=app-config`）—— 只断言第 1 条会让「把整条链掐断」也通过，
   那正是删掉传递时最容易犯的错。侧车由 z42vm 自己从 app 文件推导。

工程里两个 profile 都声明了：`z42 run` 用哪一个不是这个用例要钉的事。
