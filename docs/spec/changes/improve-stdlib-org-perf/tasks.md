# Tasks: 标准库改进——interop 收缩 + 性能

> 状态：🟡 进行中 | 创建：2026-08-03
> 性质：多阶段总纲。本 change 只做「规则文档 + B0」；A/B 各相位后续各自开 change。

## 进度概览
- [x] 阶段 0: 探索 + 事实校正（zpkg 已跨平台字节相同）+ 方向确立
- [x] 阶段 1: interop 收缩规则落文档
- [x] 阶段 2: B0 基准前置（字符串重 + 字典重场景）
- [ ] 阶段 3: 验证 + 归档

## 阶段 1: 规则文档
- [x] 1.1 `docs/design/stdlib/organization.md` 新增「平台边界库 vs 全平台共享库」节 + 改 TL;DR 规则 #3
- [x] 1.2 `src/libraries/README.md §2` 改写为两层模型 + Script-First/最小化/单一声明点/升级阶梯

## 阶段 2: B0 基准
- [x] 2.1 `bench/scenarios/07_string_heavy.z42`（IndexOf/Contains/Substring + O(n²) concat；det 输出 5175）
- [x] 2.2 `bench/scenarios/08_dict_heavy.z42`（string-key insert/lookup；det 输出 24656667）
- [x] 2.3 `bench/README.md` 场景清单登记 07/08
- [x] 2.4 编译 + 运行验证（interp+jit 均 ≥50ms，输出确定）

## 阶段 3: 验证与归档
- [ ] 3.1 `xtask bench --quick`（或全量）跑通含 07/08，e2e.json 产出正常
- [ ] 3.2 文档同步核对（触发矩阵：bench/README 功能索引已更新；organization.md 为 book 迁移期 design doc）
- [ ] 3.3 归档 + PR

## 后续相位（各自开 change，此处仅索引）
- [ ] A1 去重 cross-cutting 原语进 core
- [ ] A2 math/time intrinsic 归 core（含 bootstrap-seed 评估）
- [ ] A3 能力库 interop 单 sink + 最小导出
- [ ] A4 编译器支撑库标注 toolchain 子层
- [ ] B1 调用路径去锁去分配（perf-vm-iteration Ph1）
- [ ] B2 每对象 Mutex（Ph2）
- [ ] B3 intrinsic 表 + 去虚化
- [ ] B4 native 批量内建（gated on B1–B3 不达标）
- [ ] B5 集合算法尾巴

## 备注
- B0 workload 定标：07 interp≈486ms/jit≈326ms；08 interp≈168ms/jit≈300ms（均 >50ms hyperfine floor）。
- 单跑验证命令（复用已构建工具链，Z42_LIBS=alllibs flat）：
  `z42vm <driver> --mode interp -- --emit-zbc <src> <zbc>` 后 `z42vm <zbc> <ns>.Main --mode interp|jit`。
