# WIP 状态（feat/multicast-base-class，堆叠于 feat/generic-base-subst）

## 已验证
- MulticastDelegate<THandler> 基类（双通道 + 订阅/退订/计数/快照 4 helper，快照方法 public）
- 三族 26 类 → 继承薄壳（4541 行 → 2416 行 + 基类 220 行）
- 同包场景全通：stash 舞步（暂存瘦身文件→自举带补丁编译器→还原→build stdlib）下 stdlib 编译过
- 生成器 /tmp/slim_multicast.py（哨兵法 + func 族 handler/token 型参表 = 声明型参表原样）

## 阻塞（按序）
1. PR-A feat/generic-base-subst 先合 + nightly 重发（种子编不动瘦身后的 stdlib——自举鸡生蛋）
2. 跨包缺口：ImportedSymbolLoader 重建的类无 BaseRef 实参（ExportedClassZ 只有 BaseFqn 裸名）
   → 测试文件消费 stdlib 的 MulticastAction 时 Subscribe(THandler) 裸签名误报
   → 需要 ExportedClassZ.+BaseTypeArgs（ExportedTypeExtractor 从 AST BaseRef 拼写）+
     ImportedSymbolLoader._fillClass 重建 BaseRef NamedType + TSIG 序列化（可能 format bump）
3. protected 经泛型基类在 stdlib 场景 E0404（探针单文件过、stdlib 挂——未深挖，快照 public 绕开）

## 复现配方
git stash push src/libraries/z42.core/src/Delegates/ && rm -rf artifacts/build/compiler artifacts/build/libraries
./xtask build all && git stash pop && rm -rf artifacts/build/libraries && ./xtask build stdlib
