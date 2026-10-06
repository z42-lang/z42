# publish-apphost-sidecar

验桌面发布整条链：apphost stub 的 patch、macOS 重签名、发布出的程序按本地优先找到已安装的 SDK。

程序打印自己的 `mode` 旋钮来自哪一层。`[profile.release.runtime] mode` 由 `z42c build` 烤进
`app.runtimeconfig.toml`；`z42 publish` 必须把这个侧车带进部署布局，apphost 再把它交给 z42vm。
断言 `MODE_SRC=app-config` 时这条链上**任何一环**断了都会红 —— 工程声明的设置在用户真正发布的形态里
静默失效，是这个用例要防的事。

`[platform.desktop] apphost = true` 是 `publish` 出 apphost 的准入条件。

已安装形态的 SDK 用 `copy` 把整个包拷成 `ahome/`（真目录，不是符号链接）：跨平台行为一致、不需要 symlink 特权，
也更贴近用户 `Z42_HOME` 下的真实形态。代价是每次拷一份包（几十 MB）。
