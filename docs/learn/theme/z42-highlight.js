// highlight.js 没有 z42 语法：z42 的词法与 C# 相近，借 C# 高亮。
// mdBook 已对页面代码块跑过一遍高亮；这里把 z42 注册为 csharp 的别名后重高亮 z42 代码块。
(function () {
  if (typeof hljs === "undefined") { return; }
  if (hljs.registerAliases) { hljs.registerAliases(["z42"], { languageName: "csharp" }); }
  document.querySelectorAll("code.language-z42").forEach(function (block) {
    block.textContent = block.textContent;   // 去掉上一遍（未识别语言）留下的标记
    if (hljs.highlightElement) { hljs.highlightElement(block); } else { hljs.highlightBlock(block); }
  });
})();
