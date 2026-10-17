<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.GCHandleType

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/GC/GCHandle.z42:4`

GC 句柄模式：Weak = 弱引用（目标可被回收，Target 返回 null）；Strong = 强引用（anchor 目标直到 Free()）。

```z42
public enum GCHandleType {
    Weak
    Strong
}
```

