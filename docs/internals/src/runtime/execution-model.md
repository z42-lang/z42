# Execution Model: Interpreter, JIT, and AOT

> This document describes how z42 code is executed: the bytecode-first design, three execution modes (Interpreter / JIT / AOT), and the mechanisms for mixed-mode execution.

---

## Overview

z42 never compiles **directly to machine code**. Instead, the pipeline is:

```
Source Code
    ↓
[Parser] → AST
    ↓
[Type Checker] → Bound AST
    ↓
[IR Codegen] → IR (in-memory)
    ↓
[Bytecode Emitter] → .zbc (Bytecode File)
    ↓
    ┌──────────┬──────────┬──────────┐
    ↓          ↓          ↓          ↓
[Interpreter] [JIT]    [AOT]    [Package]
  (Execute      (Hot     (Offline   (.zpkg
   directly)   compile) compile)    dist)
```

**Key advantage:** Bytecode is the universal intermediate representation. A single `.zbc` file can be executed in any mode, or even distributed as part of a package and re-compiled by the host.

---

## Bytecode Format (.zbc)

This document focuses on **execution mode dispatch**. For canonical references see:

- **Instruction set** (opcodes / type tags / semantics): [`ir.md`](../formats/ir.md)
- **Wire format** (file header / section layout / encoding details): [`zbc.md`](../formats/zbc.md)

Three design principles relevant to mode dispatch:

- **Register-based** (not stack-based) — easier for both interpreter dispatch and JIT compilation
- **Linear bytecode stream with block-level jumps** — all jumps land on opcode boundaries; CFG made explicit by block parameters
- **Each instruction carries a type tag** — interpreter and JIT can dispatch without type inference

---

## Execution Mode 1: Interpreter

### How It Works

The interpreter is a **register-based virtual machine** written in Rust.

```rust
// src/runtime/src/exec_instr.rs (simplified)

pub fn execute_instruction(
    vm: &mut VM,
    instr: &Instruction,
    frame: &mut CallFrame,
) -> Result<(), VMError> {
    match instr.opcode {
        Opcode::LoadConst => {
            let val = instr.operand::<i64>(0)?;
            frame.regs[instr.dest_reg] = Value::Int(val);
        }
        Opcode::Add => {
            let left = frame.regs[instr.src1_reg].as_i64()?;
            let right = frame.regs[instr.src2_reg].as_i64()?;
            frame.regs[instr.dest_reg] = Value::Int(left + right);
        }
        // ... many more opcodes
    }
    Ok(())
}
```

### Performance Characteristics

| Metric | Target | Notes |
|--------|--------|-------|
| **Dispatch latency** | ≤ 5 cycles/instr | Cache-friendly; minimal branch misprediction |
| **Startup time** | < 50ms | No compilation; direct bytecode load & execute |
| **Memory overhead** | Bytecode size | No compiled code cache |
| **First execution** | Fast | Ideal for short-lived scripts, REPL, eval() |

### Use Cases

- **REPL & eval():** Dynamic code execution (no compilation delay).
- **Hot-reload scripts:** Rapid iteration without VM restart.
- **Embedded/lightweight:** When binary size and startup time matter more than throughput.
- **Fallback:** Code that's not worth JIT-compiling (cold paths, rarely-called functions).

---

## Execution Mode 2: JIT (Just-In-Time)

### How It Works

The JIT compiler monitors which functions are executed frequently and compiles them to native machine code.

```
z42 Code
  ↓
[Bytecode] ← Interpreter executes, counts calls
  ↓
[Hot function detected] (e.g., called 10,000 times)
  ↓
[JIT Compiler] (Cranelift backend)
  ↓
[Native Code] (x64 machine code)
  ↓
[Execution] Jumps directly to native code
```

### Compilation Strategy

- **Lazy compilation:** Functions are compiled when first needed, not upfront.
- **Tier-up:** Code starts in interpreter, JIT compiles when hot.
- **Adaptive:** Tier-up threshold adjusts based on VM load.
- **Deoptimization:** Speculative optimizations can be reverted if assumptions break (e.g., a monomorphic call site becomes polymorphic).

### Performance Characteristics

| Metric | Target | Notes |
|--------|--------|-------|
| **Compilation latency** | < 100ms per function | Cranelift is fast; suitable for online compilation |
| **Compiled code perf** | ≥ C# / Java | Competitive with other managed runtimes |
| **Warmup time** | ~ 1-2 seconds | Code reaches peak performance after warm-up |
| **Memory overhead** | 2-4x bytecode | Compiled code is larger than bytecode |
| **Tier-up overhead** | Transparent | User code doesn't change; tier-up happens invisibly |

### Optimization Opportunities (L2+)

- **Inline caching:** Cache method lookup results; revert if type changes.
- **Constant folding:** Evaluate constant expressions at compile time.
- **Dead code elimination:** Remove unreachable branches.
- **Loop unrolling:** Unroll small loops.
- **Speculative inlining:** Inline frequently-called functions; deoptimize if assumption breaks.

### Use Cases

- **Server workloads:** Long-running servers where warmup is acceptable.
- **Game logic:** Numeric code (physics, AI) that benefits from JIT optimization.
- **Bulk data processing:** Pipelines that run the same code repeatedly.

---

## Execution Mode 3: AOT (Ahead-Of-Time)

### How It Works

Offline compilation to native code (via LLVM), producing a standalone executable.

```
z42 Bytecode (.zbc)
  ↓
[AOT Compiler] (LLVM backend)
  ↓
[Optimization Passes] (vectorization, unrolling, etc.)
  ↓
[Object Files] (.o, architecture-specific)
  ↓
[Linker] (link with Rust runtime, libc)
  ↓
[Native Executable]
```

### Compilation Strategy

- **Full program optimization:** All code is compiled upfront; optimizer can see the entire program.
- **LTO (Link-Time Optimization):** Cross-module inlining, whole-program optimization.
- **No interpreter overhead:** Direct machine code execution.

### Performance Characteristics

| Metric | Target | Notes |
|--------|--------|-------|
| **Compile time** | 5-30s per binary | Acceptable for offline builds |
| **Execution perf** | Peak C performance | Full LLVM optimization; no interpreter |
| **Binary size** | 50-200 MB (unstripped) | Significant, but acceptable for servers |
| **Startup time** | < 100ms | No JIT warmup |
| **Memory overhead** | Compiled code + VM state | High, but predictable |
| **Latency** | Stable | No GC pauses in JIT tier-up |

### Use Cases

- **Latency-critical systems:** Hard real-time requirements (financial systems, trading, robotics).
- **Embedded firmware:** where binary size or startup is critical.
- **Production servers:** Predictable latency profile.
- **Containerized deployment:** Pre-compiled images reduce container startup time.

---

## Mixed-Mode Execution

### Declaration

Execution mode is declared per **namespace**:

```z42
[ExecMode(Mode.Interp)]        // Always interpreted
namespace Scripts.Config;

[ExecMode(Mode.Jit)]           // JIT compiled
namespace Engine.Render;

[ExecMode(Mode.Aot)]           // AOT compiled (only in standalone binaries)
namespace Core.Crypto;

// Default (determined by VM startup configuration)
namespace App.Logic;
```

### Cross-Mode Calls

Calls between namespaces in different modes are **transparent**:

```z42
// In Scripts.Config (Interp mode):
void LoadConfig() {
    Engine.Render.ComputeViewMatrix();  // ← calls into JIT-compiled code
}

// In Engine.Render (JIT mode):
void ComputeViewMatrix() {
    Core.Crypto.HMAC(key, data);        // ← calls into AOT code
}
```

**How it works:**

- **Interp → JIT:** Jump to the JIT-compiled function (same as calling any other function).
- **JIT → AOT:** Direct jump (no thunk needed).
- **Dynamic dispatch:** Virtual method calls work correctly regardless of mode (resolved via vtable).

### VM Startup Configuration

```rust
// When starting the VM:
let vm = VM::new(VMConfig {
    default_exec_mode: ExecutionMode::Jit,  // Default mode
    inline_cache_size: 16384,                 // JIT optimization budget
    interpreter_threshold: 10000,             // Hot-function threshold
    enable_aot: false,                        // AOT only in standalone builds
});
```

### Performance Implications

- **No mode switching overhead:** Once a function is decided (interp vs. JIT vs. AOT), calls are cheap.
- **VM startup:** AOT mode available only in standalone binaries (expensive to compile everything).
- **Gradual optimization:** Start in Interp (fast startup), tier-up to JIT as code gets hot.

---

## Dynamic Execution: eval()

### Scenario

```z42
string code = "var x = 10; Console.WriteLine(x);";
VM.Eval(code);  // Compile and execute immediately
```

**Pipeline:**

```
Source String
  ↓
[Parser] → AST
  ↓
[Type Checker]
  ↓
[IR Codegen]
  ↓
[Bytecode Emitter] (ephemeral bytecode, not written to disk)
  ↓
[Interpreter] (execute directly)
```

**Performance:** Compile + execute latency is ~50–200ms depending on code size. Suitable for configuration, testing, REPL.

### Constraints

- **No JIT:** eval()'d code always runs in interpreter mode (no opportunity for optimization).
- **Type safety:** Type checking happens at eval time; type errors throw exceptions.
- **Scope:** Eval'd code runs in a dedicated namespace; cannot directly modify the caller's local variables (but can access module-level globals).

---

## Hot Reload Mechanism

### How It Works

When `[HotReload]` is enabled (only with `[ExecMode(Interp)]`):

```z42
[HotReload]
[ExecMode(Mode.Interp)]
namespace Game.Scripts;

void OnUpdate(float dt) {
    // Version 1
}
```

**Step 1: Develop** — Edit the source file and recompile to bytecode.

**Step 2: Reload** — Call `VM.Reload("game.scripts.zbc")`.

**Step 3: Next call** — The next call to `OnUpdate()` executes the new version.

**Implementation:**

```rust
// src/runtime/src/hot_reload.rs (simplified)

pub fn reload_module(vm: &mut VM, path: &Path) -> Result<(), Error> {
    let bytecode = std::fs::read(path)?;
    let new_module = parse_zbc(&bytecode)?;
    
    // Install new functions
    for (func_name, func_bytecode) in new_module.functions {
        vm.function_table.insert(func_name, func_bytecode);
    }
    
    // Existing callsites now call the new version
    Ok(())
}
```

### Constraints

- **Signature changes not allowed:** If the signature of a hot-reloaded function changes (parameters, return type), compilation fails.
- **No JIT compatibility:** AOT and JIT code cannot hot-reload (too expensive to recompile machine code).
- **Interp only:** Only available in `[ExecMode(Mode.Interp)]`.
- **State preservation:** Local variables, open file handles, etc. are preserved; only the function body changes.

### Use Cases

- **Game scripting:** Reload level scripts or AI without restarting the engine.
- **Server scripting:** Reload business logic without dropping connections.
- **REPL / interactive tools:** Iterate on code without restart.

---

## Bytecode Optimizations

### String Pooling

All string literals are deduplicated in a string pool; bytecode stores indices, not inline strings.

```z42
var s1 = "hello";
var s2 = "hello";
var s3 = "world";
```

**Bytecode:**

```
LoadString r0, 0        # r0 = pool[0] ("hello")
LoadString r1, 0        # r1 = pool[0] ("hello") ← reused!
LoadString r2, 1        # r2 = pool[1] ("world")
```

**Benefit:** Bytecode is typically 40–60% smaller than source.

### Constant Folding

Simple constant expressions are evaluated at compile time:

```z42
int x = 10 + 20;        // Compiled to: LoadConst r0, 30
double pi = 3.14159;    // Compiled to: LoadConst r0, 3.14159
```

### Function Inlining (JIT)

The JIT compiler can inline small, frequently-called functions:

```z42
int Double(int x) => x * 2;

int result = Double(5);  // JIT may expand to: result = 5 * 2
```

---

## Performance Tuning

### Choosing the Right Mode

| Workload | Recommended Mode | Reason |
|----------|------------------|--------|
| REPL, eval(), scripts | **Interp** | Fast startup, no compilation |
| Server (long-lived) | **JIT** (default) | Warmup acceptable; peak perf matters |
| Real-time systems | **AOT** | Predictable latency, no GC pauses during JIT |
| Game engines | **Mixed** | Interp for scripts, JIT/AOT for engine |
| Embedded devices | **Interp** | Small bytecode, minimal memory |

### Profiling & Optimization

The VM exposes hooks for profiling:

```z42
// (L3 feature)
[Observable("__profile_enter", "__profile_exit")]
void HotFunction() {
    // VM calls __profile_enter on entry, __profile_exit on exit
}
```

External profilers can attach to these hooks and generate flame graphs, identifying JIT candidates.

---

## Related Documents

- [philosophy.md](../philosophy.md) — Bytecode-first design principle
- [ir.md](../formats/ir.md) — Bytecode instruction set reference
- [hot-reload.md](hot-reload.md) — Hot update mechanism details
