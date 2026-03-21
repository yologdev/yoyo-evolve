// Client-side mermaid rendering for mdbook
// mdbook doesn't natively support mermaid diagrams — this script loads
// mermaid.js from CDN and converts ```mermaid code blocks into rendered SVGs.

(function () {
  // Find all mermaid code blocks: <pre><code class="language-mermaid">...</code></pre>
  var codeBlocks = document.querySelectorAll("pre code.language-mermaid");
  if (codeBlocks.length === 0) return;

  // Replace each code block with a mermaid div before loading the library
  var diagrams = [];
  codeBlocks.forEach(function (codeEl, i) {
    var pre = codeEl.parentElement;
    var div = document.createElement("div");
    div.className = "mermaid";
    div.textContent = codeEl.textContent;
    diagrams.push(div);
    pre.parentElement.replaceChild(div, pre);
  });

  // Dynamically load mermaid from CDN
  var script = document.createElement("script");
  script.src =
    "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";
  script.onload = function () {
    // Detect dark theme (mdbook uses class on html element)
    var isDark =
      document.documentElement.classList.contains("coal") ||
      document.documentElement.classList.contains("navy") ||
      document.documentElement.classList.contains("ayu");

    mermaid.initialize({
      startOnLoad: false,
      theme: isDark ? "dark" : "default",
    });
    mermaid.init(undefined, ".mermaid");
  };
  document.head.appendChild(script);
})();
