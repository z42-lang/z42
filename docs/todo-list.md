1. z42 build 如果编译某个项目要能先编译依赖的项目
3. --versity 内置到std.cli 
4. 增量和并发编译
9. programs对比命令行自动注册，而不是spawn process对比性性能，看看哪个更快和多线程的问题
10. package发布要不要考虑剥离调试符号，剥离之后那怎么下载应用
11. 0.4.0版本：1.repl 2.playground 3.toolchain：z42b 4.runtime改进：1)component 2)host/hostrun/main统一，不同平台共享简化 5.不同平台的测试流程构建 6.z42 bench和流程完善应用 7.z42c基础库入标准库libraries抽象封装：metadata，ir等 7.性能改进：libraries和runtime，为开发提效， 8.libraries模块划分整理 9.book整理和内容补充
12. benchmarks构建：两部分：1.每天定时跑，有问题就提issuse 2.和其他语言对比 3.补充测试用例
13. 联合类型，即组合类型的快速访问
14. examples：配合book重新整理，配合playground，从安装，到hello world，再到工程，语法。
15. zaia: 
16. 测试用例梳理：区分runtime，不同运行模式，不同层级，比如toolchain都是host平台，加速ci减少不必要的流程
17. 改名：标准库区分std，z42，z42c，z42b等，z42vm改成z42r，z42.ir改成z42.package
18. 调试器：命令行，调试协议，vscode/vs等，组件形式，动态库