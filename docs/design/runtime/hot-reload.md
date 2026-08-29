# Hot Reload — Runtime Code Updates

> This document describes z42's hot reload mechanism: the ability to update function code at runtime without restarting the VM or losing application state. Designed for game scripting, server applications, and rapid iteration workflows.

---

## Overview

Hot reload allows updating **function code at runtime** without restarting the VM. The VM continues executing with new code on the next function call.

**Key use cases:**

- **Game scripting:** Reload AI behavior, level logic, or UI without restarting the engine.
- **Server applications:** Update request handlers, business logic, or algorithms without dropping connections.
- **Interactive tools:** REPL, notebook environments, live coding.
- **Rapid iteration:** Cut turnaround from 30–60 seconds (restart) to 1–2 seconds (reload).

---

## Enabling Hot Reload

### 1. Annotation (Recommended)

```z42
[ExecMode(Mode.Interp)]    // ← Required: interp mode only
[HotReload]                 // ← Enable hot reload
namespace Game.Scripts;

void OnUpdate(float dt) { ... }
void OnCollision(Entity a, Entity b) { ... }
```

**Rules:**

- `[HotReload]` **requires** `[ExecMode(Mode.Interp)]`.
- When enabled, the VM **watches the bytecode file** for changes and reloads automatically (or via explicit API).

### 2. Explicit API

Trigger reload programmatically:

```z42
// Reload a module immediately
VM.ReloadModule("game.scripts", bytecode);

// Or watch for file changes:
VM.WatchModule("game.scripts", "/path/to/game_scripts.zbc");
```

---

## Semantics

### Function-Level Replacement

- Hot reload replaces **functions by name**.
- After reload, **the next call** to a function sees the new implementation.
- Calls **currently on the stack** continue with the old code until the frame returns.

**Example:**

```
[Call Stack Before Reload]
├─ Main()
├─ OnFrame()
│  ├─ OnUpdate()         ← Executing old version
│  └─ PhysicsStep()

[Reload happens]

├─ OnFrame() continues...
├─ OnUpdate() returns ← Returns from old version
├─ [Next call to OnUpdate sees new version]
```

### State Preservation

| State | After Reload |
|-------|--------------|
| Module-level globals | **Preserved** (not reset) |
| Function bytecode | **Replaced** |
| Executing call frames | **Unchanged** (continue with old code) |
| Closures / lambdas | **Replaced** (next call uses new version) |
| Type definitions (class/struct) | **Not reloadable** (see constraints) |

### Hooks: Pre/Post Reload

Modules can declare hooks to save and restore state:

```z42
[HotReload]
namespace Game.Scripts;

private List<Enemy> enemies;  // Preserved across reload

static void OnBeforeReload() {
    Console.WriteLine("Saving state before reload...");
    // Save transient state if needed
}

static void OnAfterReload() {
    Console.WriteLine("Reload complete!");
    // Reinitialize if needed
}
```

The VM calls these hooks **automatically** when reloading (if they exist).

---

## Constraints

### Phase 1 Limitations

| Constraint | Reason |
|-----------|--------|
| **Interp mode only** | JIT/AOT code is compiled to machine code; hot reload requires bytecode. |
| **No signature changes** | Callsites are compiled with the old signature; changing it breaks callers. |
| **No new/deleted functions** | Can only replace existing functions; adding new ones requires VM restart. |
| **No type changes** | Class/struct definitions cannot be reloaded; changing fields breaks existing instances. |
| **No cross-module dependencies** | Module A can't assume Module B's types changed if B is reloaded (Module B's types are immutable). |

### Example: What Breaks

```z42
// ✗ NOT allowed: signature change
[Before] void OnUpdate(float dt) { ... }
[After]  void OnUpdate(float dt, int frame) { ... }
// Error: reload fails; old callsites expect 1 argument

// ✗ NOT allowed: new function (without restart)
[Before] class Player { public int HP; }
[After]  class Player { public int HP; public int Mana; }  // ← Type changed!
// Error: existing instances don't have Mana field

// ✓ OK: change function body
[Before] void OnUpdate(float dt) { Console.WriteLine("v1"); }
[After]  void OnUpdate(float dt) { Console.WriteLine("v2"); }
// Success: next call sees new version
```

---

## Implementation Details

### 1. Function Table

The VM maintains a **function code table**:

```rust
// src/runtime/src/metadata/bytecode.rs

pub struct Module {
    functions: HashMap<String, FunctionCode>,
    // "game.scripts::on_update" → bytecode
}

// When reloading:
pub fn reload(&mut self, new_module: Module) {
    for (func_name, func_code) in new_module.functions {
        self.functions.insert(func_name, func_code);  // ← Replace
    }
}
```

### 2. Call Path

When the interpreter calls a function:

```rust
pub fn call(&mut self, name: &str, args: &[Value]) -> Result<Value> {
    let code = self.functions.get(name)?;  // ← Look up current code
    self.execute(code, args)
}
```

Because the VM **always looks up** the function code at call time, the new version is automatically used after reload.

### 3. Multi-threaded VM (L3+)

When the VM supports multiple threads:

```rust
pub struct Module {
    functions: Arc<RwLock<HashMap<String, FunctionCode>>>,
}

pub fn reload(&self, new_module: Module) {
    let mut functions = self.functions.write();  // ← Acquire write lock
    for (func_name, func_code) in new_module.functions {
        functions.insert(func_name, func_code);
    }
    // Write lock released; all threads see new code
}
```

**Guarantees:**

- Readers (threads executing functions) are not blocked by reload.
- Reload acquires a write lock (brief, < 1ms).
- New threads see the new code immediately.

---

## Best Practices

### 1. Separate Types from Logic

**Good practice:**

```z42
// types.z42 — Stable, not reloaded
public class Player {
    public int HP;
    public string Name;
}

// logic.z42 — Hot-reloaded frequently
[HotReload]
namespace Game.Logic;

public void UpdatePlayer(Player p, float dt) {
    p.HP -= 1;  // Update this logic freely
}
```

**Why:** Types don't change; logic can change. Clear separation.

### 2. Use Stable Interfaces

Define public contracts that don't change:

```z42
// interface.z42 — Not reloaded
public interface IPlayerController {
    void OnUpdate(float dt);
    void OnInput(Key k);
}

// implementation.z42 — Reloaded
[HotReload]
namespace Game.Scripts;

public class PlayerController : IPlayerController {
    public void OnUpdate(float dt) { /* changes */ }
    public void OnInput(Key k) { /* changes */ }
}
```

Callers only care about the interface; implementation can be swapped.

### 3. Avoid Callbacks Across Reload Boundary

**Problematic:**

```z42
public class Game {
    public Action<float> OnFrame;
}

[HotReload]
namespace Scripts;

void RegisterCallbacks(Game g) {
    g.OnFrame = (dt) => OnUpdate(dt);  // ← Lambda becomes stale after reload
}
```

After reload, the lambda still points to old code.

**Better:**

```z42
void RegisterCallbacks(Game g) {
    g.OnFrame = (dt) => CallByName("on_update", dt);  // ← Dispatch by name
}

// VM will call the new version of on_update
```

### 4. Log Reload Events

```z42
[HotReload]
namespace Game.Scripts;

private int reloadCount = 0;

static void OnAfterReload() {
    reloadCount++;
    Console.WriteLine("[Reload #{0}] Scripts reloaded", reloadCount);
}
```

Useful for debugging and understanding what's being reloaded.

---

## Performance

### Reload Overhead

| Phase | Time |
|-------|------|
| **Bytecode parse** | ~10–50ms (depends on file size) |
| **Type-check** | ~5–20ms |
| **Lock acquisition** | < 1ms |
| **Function replacement** | ~1–5ms (depends on # of functions) |
| **Total** | ~30–100ms |

**Conclusion:** Reload is orders of magnitude faster than VM restart (which takes seconds).

### Runtime Overhead

Hot reload has **zero runtime cost** when not used. When enabled:

- Function table lookup is a single hash table lookup (~O(1)).
- No branch or performance penalty compared to static dispatch.

---

## Limitations & Future Work

### Not Supported (L2)

- **Type definition changes** — Can't modify class fields, add methods, or change inheritance.
- **Selective reload** — Must reload the entire module; can't reload individual functions.
- **Graceful fallback** — If new code fails type-check, old version remains; no automatic rollback.
- **JIT/AOT reload** — Only interpreter mode supported.

### Future (L3+)

- **Type-safe reload** — Reload with struct field additions (automatically zero-initialize new fields on existing instances).
- **Diff-based reload** — Only transmit changed bytecode (for remote scripting).
- **Async reload** — Reload without pausing the VM (for responsive servers).
- **Persistent state migration** — Automatically migrate module-level globals when type signatures change.

---

## Example: Game Engine Integration

### Game Script (Reloadable)

```z42
[ExecMode(Mode.Interp)]
[HotReload]
namespace Game.Scripts;

private float aiAggression = 0.5f;

public void UpdateAI(NPC npc, float dt) {
    if (player.Distance < 5.0f) {
        npc.Attack(aiAggression);
    }
}

static void OnAfterReload() {
    Console.WriteLine("AI logic reloaded");
}
```

### Engine (Stable)

```rust
// In the game engine (Rust):
fn main() {
    let mut vm = VM::new(VMConfig::default());
    let script_bytecode = std::fs::read("game_scripts.zbc")?;
    vm.load_module("game.scripts", script_bytecode)?;
    
    for frame in 0..1000000 {
        // Call reloadable script function
        vm.call("game.scripts::update_ai", &[npc, dt])?;
        // The next reload automatically uses new code
    }
}
```

### Workflow

```
1. Developer edits game_scripts.z42
2. Recompiles: z42c build game_scripts.z42.toml
3. VM detects new bytecode: [Reload] reloaded 2 functions in game.scripts
4. Next frame: UpdateAI() uses new code
5. Result: < 2 second iteration loop (vs. 30s with restart)
```

---

## Related Documents

- [philosophy.md](../philosophy.md) — Dynamic execution principle
- [execution-model.md](execution-model.md) — Interpreter mode (required for hot reload)
- [language-overview.md](../language/language-overview.md) — `[HotReload]` and `[ExecMode]` syntax
