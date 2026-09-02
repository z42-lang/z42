# Spec: Array range / offset / paired 重载

## ADDED Requirements

### Requirement: range 排序

#### Scenario: 排序子区间，区间外不动
- **WHEN** `Sort<int>(new int[]{5,4,3,2,1}, 1, 3)`（排 index 1..3）
- **THEN** 结果为 `{5,2,3,4,1}`（index 0 与 4 保持不变，中段升序）

#### Scenario: 越界抛异常
- **WHEN** `Sort<int>(xs, 2, 5)` 且 `2+5 > xs.Length`
- **THEN** 抛 `Exception`

### Requirement: range 反转 / 填充

#### Scenario: 反转子区间
- **WHEN** `Reverse<int>(new int[]{1,2,3,4,5}, 1, 3)`
- **THEN** 结果 `{1,4,3,2,5}`

#### Scenario: 填充子区间
- **WHEN** `Fill<int>(new int[5], 7, 1, 2)`
- **THEN** index 1,2 为 7，其余为默认值

### Requirement: 偏移拷贝

#### Scenario: 跨数组偏移拷贝
- **WHEN** `Copy<int>(src={1,2,3,4}, 1, dst=new int[4], 0, 2)`
- **THEN** `dst == {2,3,0,0}`

#### Scenario: 同数组重叠区正确处理
- **WHEN** `Copy<int>(a={1,2,3,4,5}, 0, a, 1, 4)`（自身右移一位）
- **THEN** `a == {1,1,2,3,4}`（不被覆盖污染）

### Requirement: range 查找

#### Scenario: IndexOf 从 startIndex 起找
- **WHEN** `IndexOf<int>({5,1,5,2,5}, 5, 1)`
- **THEN** 返回 2（跳过 index 0）

#### Scenario: IndexOf 限定 count
- **WHEN** `IndexOf<int>({5,1,5,2,5}, 5, 1, 1)`（从 1 起找 1 个）
- **THEN** 返回 -1（区间 [1,2) 内无 5）

#### Scenario: LastIndexOf 从 startIndex 逆向
- **WHEN** `LastIndexOf<int>({5,1,5,2,5}, 5, 2)`（从 index 2 逆向）
- **THEN** 返回 2

### Requirement: range / comparer 二分查找

#### Scenario: range 二分命中
- **WHEN** `BinarySearch<int>({9,1,3,5,7,9}, 1, 4, 5)`（在 [1,5) 内找 5）
- **THEN** 返回该元素下标

#### Scenario: comparer 二分未命中返回 ~插入点
- **WHEN** `BinarySearch<int>({1,3,5,7,9}, 4, (a,b)=>a-b)`
- **THEN** 返回 `-3`（插入点 2 → ~2）
