// 侧边目录：分部标题（SUMMARY 里的 `# 标题`）可点击折叠 / 展开。
// 嵌套子页的折叠由 mdBook 自带的 [output.html.fold] 负责，这里只管分部。
// 三本书（learn / reference / internals）各放一份相同的文件，改动时同步。
//
// 默认：只展开当前页所在的分部；之后沿用读者上次的选择（localStorage，不可用时退化为默认）。
(function () {
  var STORE = "z42-toc-collapsed:" + new URL(window.path_to_root || "", location.href).pathname;

  function load() {
    try { return JSON.parse(localStorage.getItem(STORE)); } catch (e) { return null; }
  }
  function save(titles) {
    try { localStorage.setItem(STORE, JSON.stringify(titles)); } catch (e) { /* 无存储：只在本页生效 */ }
  }

  function injectStyle() {
    var s = document.createElement("style");
    s.textContent =
      ".sidebar .part-title{cursor:pointer;user-select:none}" +
      ".sidebar .part-title::before{content:'\\25BE\\00a0';opacity:.6}" +
      ".sidebar .part-title.z42-collapsed::before{content:'\\25B8\\00a0'}" +
      ".sidebar .z42-toc-hidden{display:none !important}";
    document.head.appendChild(s);
  }

  function init(ol) {
    var groups = [], cur = null;
    Array.prototype.forEach.call(ol.children, function (el) {
      if (el.classList.contains("part-title")) { cur = { title: el, items: [] }; groups.push(cur); }
      else if (cur) { cur.items.push(el); }
    });
    if (!groups.length) { return; }

    var stored = load();
    var collapsed = Array.isArray(stored) ? stored : null;
    groups.forEach(function (g) {
      g.name = g.title.textContent.trim();
      g.hasActive = g.items.some(function (el) { return el.querySelector("a.active"); });
    });
    // 无存储：除当前页所在分部外全部折叠；有存储：按存储，但当前页所在分部一律展开
    var state = groups.map(function (g) {
      if (g.hasActive) { return false; }
      return collapsed ? collapsed.indexOf(g.name) >= 0 : true;
    });

    function apply(i) {
      var g = groups[i];
      g.title.classList.toggle("z42-collapsed", state[i]);
      g.title.setAttribute("aria-expanded", String(!state[i]));
      g.items.forEach(function (el) { el.classList.toggle("z42-toc-hidden", state[i]); });
    }
    function persist() {
      save(groups.filter(function (_, i) { return state[i]; }).map(function (g) { return g.name; }));
    }

    groups.forEach(function (g, i) {
      g.title.setAttribute("role", "button");
      g.title.setAttribute("tabindex", "0");
      function toggle() { state[i] = !state[i]; apply(i); persist(); }
      g.title.addEventListener("click", toggle);
      g.title.addEventListener("keydown", function (ev) {
        if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); toggle(); }
      });
      apply(i);
    });
  }

  // 目录由 mdBook 的脚本注入，晚于本脚本：等到 `.part-title` 出现再挂载。
  function findList() {
    var first = document.querySelector(".sidebar .part-title");
    return first && first.parentElement;
  }
  function start() {
    var ol = findList();
    if (ol) { injectStyle(); init(ol); return true; }
    return false;
  }
  if (start()) { return; }
  var obs = new MutationObserver(function () {
    if (start()) { obs.disconnect(); }
  });
  obs.observe(document.documentElement, { childList: true, subtree: true });
  setTimeout(function () { obs.disconnect(); }, 10000);
})();
