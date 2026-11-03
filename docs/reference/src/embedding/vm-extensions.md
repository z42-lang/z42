# Rust VM 扩展状态

直接使用 Rust `z42` crate 的宿主，可以在创建 VM 前提供自己的状态。每个状态类型必须满足 `Any + Send + Sync`，
由 VM 持有所有权；同一类型重复登记会返回错误，原状态保持不变。需要多份同类服务时使用不同的新类型。

```rust
use z42::{pal::fs::default_vfs, vm::ext::VmExtensions, vm_context::VmContext};

fn main() -> anyhow::Result<()> {
    let mut extensions = VmExtensions::new();
    extensions.register(String::from("tenant"))?;
    let ctx = VmContext::with_extensions(None, default_vfs(), extensions);
    assert_eq!(ctx.ext::<String>().map(String::as_str), Some("tenant"));
    Ok(())
}
```

`with_extensions` 的三个参数依次是可选入口 `Module`、该 VM 的 `Vfs` 和创建前的登记容器；
`Some(module)` 使用该模块，`None` 与无模块的 `VmContext::new()` 一样。函数返回 `Pin<Box<VmContext>>`。
原有便捷构造函数默认创建空的扩展状态。

创建函数消耗登记容器，之后只能用 `ext::<T>() -> Option<&T>` 借用查询；没有登记该类型时返回 `None`。
查询不提供注册、替换或移除接口；状态内部需要更新的数据由其自身的锁或原子类型保护。
同一 core 的 worker contexts 共享状态，不同 VM 的登记互相独立。宿主主动登记同一个 `Arc` 的克隆时，
它指向的数据仍按 Rust 的 `Arc` 规则共享。

状态存活到最后一个 core 引用释放，晚于该 VM 的 heap 和引擎释放，并按登记逆序销毁。
未交给 VM 的登记容器在自身销毁时也会释放状态。容器不自动把其中的 z42 托管值变成 GC 根；
宿主须在 VM 存活期间按现有根管理接口登记、释放所持的托管值。
