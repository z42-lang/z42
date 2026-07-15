# Tasks: HTTP chunked chunk-size i32 溢出修复（3 处 _parseHex）

> 状态：🟢 已完成 | 创建：2026-07-16 | 完成：2026-07-16 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32）实施+验证。

**变更说明：** review §2.2——chunked transfer-encoding 的 chunk-size 十六进制解析用带符号 int
累加（`result = result*16 + digit`），8+ 位 hex 溢出 i32 回绕：`0x100000000`（2^32）回绕成 0 →
被当「body 结束」静默截断；其它值可回绕成错误正尺寸。改 long 累加 + 显式拒绝 >int.MaxValue（返 -1，
解码器抛 HttpProtocolException）。`_parseHex` 在 HttpClient / _HttpRequestParser / _HttpBodyStream
**三份重复**，三处同修。

**原因：** review §2.2——chunk-size 带符号 int 累加正向 wrap 放行错误尺寸/静默截断。

**文档影响：** 无对外 API 变化；行为「溢出 chunk-size 静默截断→协议错误拒绝」。net README 无需改；
      无 book 变更。

- [x] 1.1 `HttpClient.z42` `_parseHex`：long 累加 + `>2147483647L` 返 -1
- [x] 1.2 `_HttpRequestParser.z42` `_parseHex`：同上
- [x] 1.3 `_HttpBodyStream.z42` `_parseHex`：同上
- [x] 1.4 回归测试 `http_chunked.z42`：chunk-size `100000000`（2^32）经真实解码路径 → 抛异常拒绝
- [x] 1.5 GREEN：worktree 内 `xtask test` 全绿（net 50 文件）
- [x] 1.6 归档 + 释放 ACTIVE.md stdlib 锁

## 备注
- 三份 `_parseHex` 重复本身是 §3.7 编译器 typed-overload / 跨文件复用债的一个实例；收敛留独立 refactor。

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「溢出拒绝」。README 无需改；无 book 变更
