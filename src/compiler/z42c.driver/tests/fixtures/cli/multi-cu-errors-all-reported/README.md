# multi-cu-errors-all-reported — z42c build 全 CU 错误呈现

unify-driver-on-build-session K2：`z42c build` 原先只报**第一个**有错 CU 即返回（与
BuildSession.Run ⑦ 的逐 CU 全报分叉）——两个文件各有错时只看见 a.z42 的，修完重建才见
b.z42 的。本夹具钉对齐后的行为：两个坏文件的错误**一并**列出。
