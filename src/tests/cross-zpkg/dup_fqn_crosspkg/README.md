# dup_fqn_crosspkg — 跨包同 FQN 门（E0601）

**这道门盯的是：两个互不依赖的包声明了同一个全限定名时，编译器必须说话。**

`target`（`demo.dupalpha`）与 `ext`（`demo.dupbeta`）各声明一个 `Demo.DupNs.Widget`——
不是同短名跨 ns（那归 E0456），是**同一个 FQN**。消费方 `main` 引用它。

## 修前行为（实测，非推断）

编译 **rc=0、零诊断**；字母序靠前的 `demo.dupalpha` 赢，`demo.dupbeta` 的 `Widget` 连同
全部成员从未存在过。运行期倒是会 warn（`duplicate type ... keeping first-loaded`，默认打 stderr），
**全哑的只有编译期**。

更糟的形态：给 beta 的 `Widget` 加一个 alpha 没有的成员再调用它 →
`E0401: no method 'OnlyBeta' on 'Widget'`。用户正看着 beta 的源码，诊断答非所问——
真相不是「没有这个方法」，而是「`Widget` 根本不是你以为的那一个」。

## 负例 fixture 约定

本目录用 `expected_build_error.txt`（**不是** `expected_output.txt`）：main 必须**编译失败**，
且 stderr 含该文件的内容。编过了 = 判红（诊断没响）。约定见 `../README.md`。
