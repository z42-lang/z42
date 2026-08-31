# Tasks

- [x] resolver：unresolvable builtin → UNRESOLVED（非 panic）
- [x] interp builtin：UNRESOLVED → exec_builtin(name)
- [x] jit_builtin：+name 参数，UNRESOLVED → exec_builtin(name)
- [x] translate：发射 name，去 static panic
- [x] registry：jit_builtin decl 补 name
- [x] cargo test --lib
- [ ] CI: bench + stdlib-jit + bootstrap 全绿
