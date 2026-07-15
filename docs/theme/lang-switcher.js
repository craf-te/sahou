(function () {
  function currentAndCounterpart() {
    // path_to_root is a relative path from this page to the book root (e.g. "../").
    var ptr = (typeof path_to_root !== "undefined" && path_to_root) ? path_to_root : "./";
    var root = new URL(ptr, window.location.href).href; // absolute book root, trailing slash
    var rel = window.location.href.slice(root.length);   // page path within this book
    var isJa = /\/ja\/$/.test(root);
    if (isJa) {
      var enRoot = root.replace(/ja\/$/, "");
      return { lang: "ja", en: enRoot + rel, ja: window.location.href };
    }
    return { lang: "en", en: window.location.href, ja: root + "ja/" + rel };
  }

  function build() {
    var bar = document.querySelector(".menu-bar .right-buttons") ||
              document.querySelector(".right-buttons");
    if (!bar) return;
    var info = currentAndCounterpart();
    var wrap = document.createElement("div");
    wrap.className = "sahou-lang";
    var sel = document.createElement("select");
    sel.setAttribute("aria-label", "Language");
    [["en", "🌐 English", info.en], ["ja", "🌐 日本語", info.ja]].forEach(function (o) {
      var opt = document.createElement("option");
      opt.value = o[2];
      opt.textContent = o[1];
      if (o[0] === info.lang) opt.selected = true;
      sel.appendChild(opt);
    });
    sel.addEventListener("change", function () { window.location.href = sel.value; });
    wrap.appendChild(sel);
    bar.appendChild(wrap);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", build);
  } else {
    build();
  }
})();
