# Spec: process-group opt-out for interactive forwarding

## ADDED Requirements

### Requirement: Process.ShareProcessGroup keeps the child in the caller's process group

#### Scenario: interactive REPL forward drives the tty
- **WHEN** launcher `_forwardRepl` spawns `z42i` with `Stdin(Inherit)` + `.ShareProcessGroup()` under a real tty
- **THEN** the interactive vm shares the launcher's (foreground) process group, shows the `>>>` prompt, and evaluates input

#### Scenario: default keeps own process group (tree-kill preserved)
- **WHEN** `Process.Run()` is called without `ShareProcessGroup()`
- **THEN** the child is placed in its own process group and a run-timeout group-kills the whole tree (unchanged behavior)

### Requirement: arg 14 is read defensively (no arity coupling)

#### Scenario: old 14-arg call on a new VM
- **WHEN** `__process_run` is invoked with only 14 args (no `own_process_group`)
- **THEN** it behaves as `own_process_group=true` (own group), so old seed zpkgs keep working on a new VM

## MODIFIED Requirements

### Requirement: run-timeout tree-kill

**Before:** on timeout, `kill(-pid)` always group-kills (assumes child owns its group).
**After:** `kill(-pid)` only when the child owns its group; a shared-group child is killed directly (prevents killing the caller's group).

## Pipeline Steps
- [ ] VM interp（`builtin_process_run` — 无 IR/lexer/parser 变更）
