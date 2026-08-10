# Tasks: 预计算栈帧名，消除每次调用的 format+alloc

**变更说明：** interp 每次函数调用（`exec_function_body`）都 `format_frame_name(func)`（String 分配+格式化）
+ 克隆 file 字符串构造 `VmFrame`，但帧名只在**栈回溯**（异常/profiling，罕见）时用到。改为在**加载期**
（`build_block_indices`，与 branch_targets 同处）为每个 Function 预计算 `(name, file): (Arc<str>, Arc<str>)`
存入新 skip 字段 `frame_meta`，`exec_function_body` 每次调用只 **O(1) 克隆 Arc**。JIT 早已如此（FnEntry）；
本改把 interp 拉齐。手搓测试函数无 frame_meta（None）→ 回退现场格式化。
**原因：** 实测 format+alloc 占**调用密集 interp 时间的 40–60%**（fib 2.5× / OO 1.78×）。这是本轮最大杠杆。
**文档影响：** 无（纯运行期优化，行为不变；栈回溯帧名 interp==jit 一致）。

- [x] 1.1 `bytecode.rs`：Function 加 `#[serde(skip)] frame_meta: Option<(Arc<str>,Arc<str>)>`
- [x] 1.2 `loader.rs::build_block_indices`：加载期预计算 (name,file) 填入 frame_meta
- [x] 1.3 `interp/mod.rs::exec_function_body`：Some→O(1) 克隆；None→回退格式化（手搓测试函数）
- [x] 1.4 全 Function 字面构造点补 `frame_meta: None`（zbc_reader ×2 + 测试构造点）
- [x] 1.5 正确性：uncaught 栈回溯帧名正确且 interp==jit（UC.Boom(long) file:line:col）；异常 catch 正常；cargo test 897/0
- [x] 1.6 性能：fib 7655→3012ms(−60%/2.5×) / OO 5740→3221ms(−44%)；dispatch-heavy 无回退（int scan 1072 / char scan 469 不变）
- [x] 1.7 完整 GREEN：cargo test 897/0 + e2e-direct 205/208（interp+jit，=baseline 同款 3 例直跑器局限，零回退）→ PR

## 备注：与 [[interp-frame-string-cache-regresses]] 的关系
该记忆说「OnceLock 缓存帧名回退 7%」——本改**不用 OnceLock**（无每次调用的原子 check），而是加载期**急切**
预计算填 skip 字段（同 branch_targets），热路径纯 Arc 克隆。实测 dispatch-heavy **零回退**，call-heavy 2.5×。
旧结论应属 OnceLock 实现的 hot-struct 膨胀/原子开销，非「预计算无益」。记忆需更正。
