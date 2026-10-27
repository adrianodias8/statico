// Search over the search.json that `statico build` writes (build.search is on
// in statico.json). No dependencies: every query term must appear in a page;
// matches in the title rank first.
(() => {
  // Where the site lives: "/" at a domain root, "/project/" on a GitHub Pages
  // project site. search.json and the URLs inside it are relative to it.
  const declared = document.querySelector("[data-search]")?.dataset.base || "/";
  const base = new URL(new URL(declared, location.href).pathname, location.origin);
  const at = (path) => new URL(path.replace(/^\//, ""), base).pathname;
  let index = null;
  const load = () =>
    (index ??= fetch(new URL("search.json", base))
      .then((r) => (r.ok ? r.json() : []))
      .catch(() => []));

  const esc = (s) =>
    String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  const title = (t) => String(t ?? "").replace(/\s·\s[^·]*$/, "");

  function search(pages, q) {
    const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
    if (!terms.length) return [];
    return pages
      .map((p) => {
        const t = title(p.title).toLowerCase();
        const body = `${p.description ?? ""} ${p.text ?? ""}`.toLowerCase();
        if (!terms.every((w) => t.includes(w) || body.includes(w))) return null;
        // Title matches first, then how often the terms appear (capped, so
        // one long page can't drown the rest).
        const score = terms.reduce(
          (s, w) => s + (t.includes(w) ? 10 : 0) + Math.min(body.split(w).length - 1, 5),
          0,
        );
        return { p, score };
      })
      .filter(Boolean)
      .sort((a, b) => b.score - a.score)
      .map((r) => r.p);
  }

  const item = (p) =>
    `<a href="${esc(at(p.url))}"><strong>${esc(title(p.title))}</strong><span>${esc(p.description)}</span></a>`;

  document.addEventListener("DOMContentLoaded", () => {
    // On narrow screens the contents start folded away above the page.
    const toggle = document.querySelector(".index-toggle");
    if (toggle && matchMedia("(max-width: 760px)").matches) toggle.open = false;

    const form = document.querySelector("[data-search]");
    const input = form?.querySelector("input");
    const box = form?.querySelector("[data-results]");
    if (!form || !input || !box) return;

    let active = -1;
    const links = () => [...box.querySelectorAll("a")];
    const highlight = () => links().forEach((a, i) => a.toggleAttribute("aria-current", i === active));

    input.addEventListener("focus", load);
    input.addEventListener("input", async () => {
      const q = input.value.trim();
      if (!q) { box.hidden = true; return; }
      const pages = await load();
      const hits = search(pages, q).slice(0, 8);
      active = -1;
      box.innerHTML = pages.length
        ? hits.length ? hits.map(item).join("") : `<p>No pages match “${esc(q)}”.</p>`
        : `<p>Search works on the built site: run <code>statico build</code>.</p>`;
      box.hidden = false;
    });
    input.addEventListener("keydown", (e) => {
      const n = links().length;
      if (e.key === "ArrowDown" && n) { active = (active + 1) % n; highlight(); e.preventDefault(); }
      else if (e.key === "ArrowUp" && n) { active = (active - 1 + n) % n; highlight(); e.preventDefault(); }
      else if (e.key === "Enter" && active >= 0) { links()[active].click(); e.preventDefault(); }
      else if (e.key === "Escape") { box.hidden = true; input.blur(); }
    });
    document.addEventListener("click", (e) => { if (!form.contains(e.target)) box.hidden = true; });

    // The /search/ page lists every result for ?q=.
    const list = document.querySelector("[data-search-page-results]");
    const q = new URLSearchParams(location.search).get("q");
    if (list && q) {
      input.value = q;
      load().then((pages) => {
        const hits = search(pages, q);
        document.querySelector("[data-search-page]").textContent =
          `${hits.length} page${hits.length === 1 ? "" : "s"} match “${q}”.`;
        list.innerHTML = hits.map((p) => `<li>${item(p)}</li>`).join("");
      });
    }
  });
})();
