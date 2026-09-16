---
name: next-phase
description: 规划并实施 z42 编译器/VM 的下一个实现阶段。在用户说"开始下一阶段"、"实现类型检查器"、"继续推进" 时触发。
user-invocable: true
allowed-tools: Read, Grep, Glob
context: fork
agent: Plan
---

# 规划下一实现阶段

## 当前进度检查

先读取以下文件了解现状：
- [roadmap.md](../../../docs/roadmap.md) —— 迭代计划与当前焦点（CLAUDE.md 不再复制焦点）
- 语言规则：`docs/reference/`（⏳ 三书重构搬迁中，暂在 `docs/design/language/`）
- IR 指令集：`docs/internals/src/formats/ir.md`（⏳ 暂在 `docs/internals/src/formats/ir.md`）

然后扫描代码库，识别哪些内容标注了 `// TODO` 或 `// TODO:`：

```bash
grep -rn "TODO" src/
```

## 阶段优先级

按以下顺序推进，不要跳跃：

1. **类型检查器**（`Z42.Compiler/TypeCheck/`）
   - 符号表 / 作用域树
   - 类型推导（Hindley-Milner 子集）
   - 所有权检查（借用规则验证）

2. **IR Codegen**（`Z42.Compiler/Codegen/`）
   - AST → SSA IR
   - 发射 `.zbc` 二进制文件（格式见 zbc 页；⏳ 搬迁中 → `docs/internals/src/formats/zbc.md`）

3. **解释器完整实现**（`src/runtime/src/interp.rs`）
   - 完整指令集覆盖
   - 函数调用帧栈
   - 内置函数（io.println 等）

4. **JIT 后端**（Cranelift）
   - 依赖：`cranelift-jit = "0.x"` 加入 Cargo.toml
   - IR → Cranelift IR → 原生代码

## 输出格式

返回一个具体的实施计划，包含：
- 需要新建的文件列表
- 需要修改的文件列表
- 每个文件的核心接口/类型设计草案
- 估计的实现步骤顺序
