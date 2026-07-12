1. z42 build 如果编译某个项目要能先编译依赖的项目
2. z42 publish xtask 要不依赖desktop，要利用起build的流程，这样也可以简化quickstart 
3. --versity 内置到std.cli 
4. 增量和并发编译
5. 增加remote command 打印skill 方便远程查看
6. 增加定时唤醒查看ci功能
7. 编译增加z42b，z42c等版本hash,可以确保版本不同会重新编译
8. xtask里面的全部路径应该都从z42.toml中获取，而不是写死拼接
9. programs对比命令行自动注册，而不是spawn process对比性性能，看看哪个更快和多线程的问题
10. package发布要不要考虑剥离调试符号，剥离之后那怎么下载应用
11. 0.4.0版本：1.repl 2.playground 3.toolchain：z42b 4.runtime改进：1)component 2)host/hostrun/main统一，不同平台共享简化 5.不同平台的测试流程构建 6.z42 bench和流程完善应用 7.z42c基础库入标准库libraries抽象封装：metadata，ir等 7.性能改进：libraries和runtime，为开发提效， 8.libraries模块划分整理 9.book整理和内容补充