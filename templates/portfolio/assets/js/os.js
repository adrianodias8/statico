// adwebdev os: turns the portfolio page into a small operating system. The
// page is complete without this script; here we add the boot screen, windows
// that open, focus, drag and close, the clock, sound, and a terminal.
(() => {
  const root = document.documentElement;
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const phone = () => matchMedia("(max-width: 720px)").matches;
  const store = {
    get: (k) => { try { return localStorage.getItem(k); } catch { return null; } },
    set: (k, v) => { try { localStorage.setItem(k, v); } catch {} },
  };

  // ---- Sound: short square-wave blips, off unless the visitor turns it on.
  let audio = null;
  let soundOn = store.get("os.sound") === "on";
  function blip(freq = 880, ms = 40, gain = 0.03) {
    if (!soundOn) return;
    audio ??= new (window.AudioContext || window.webkitAudioContext)();
    const o = audio.createOscillator();
    const g = audio.createGain();
    o.type = "square";
    o.frequency.value = freq;
    g.gain.value = gain;
    o.connect(g).connect(audio.destination);
    o.start();
    o.stop(audio.currentTime + ms / 1000);
  }

  // ---- Menu bar: clock (Brussels time), CRT and sound switches.
  function clock() {
    const t = document.querySelector("[data-clock]");
    if (!t) return;
    const fmt = new Intl.DateTimeFormat("en-GB", { hour: "2-digit", minute: "2-digit", timeZone: "Europe/Brussels" });
    const tick = () => (t.textContent = fmt.format(new Date()));
    tick();
    setInterval(tick, 15000);
  }
  function switches() {
    const crt = document.querySelector('[data-toggle="crt"]');
    const snd = document.querySelector('[data-toggle="sound"]');
    const crtOn = store.get("os.crt") !== "off";
    root.classList.toggle("no-crt", !crtOn);
    crt?.setAttribute("aria-pressed", crtOn);
    snd?.setAttribute("aria-pressed", soundOn);
    crt?.addEventListener("click", () => {
      const on = root.classList.toggle("no-crt");
      crt.setAttribute("aria-pressed", !on);
      store.set("os.crt", on ? "off" : "on");
    });
    snd?.addEventListener("click", () => {
      soundOn = !soundOn;
      snd.setAttribute("aria-pressed", soundOn);
      store.set("os.sound", soundOn ? "on" : "off");
      blip(660, 60);
    });
  }

  // ---- Boot screen: once per visit, skippable, instant with reduced motion.
  function boot(done) {
    const screen = document.querySelector("[data-boot]");
    let seen = false;
    try { seen = sessionStorage.getItem("os.booted") === "1"; } catch {}
    if (!screen || seen || location.hash.length > 1) return done();
    const lines = [...screen.querySelectorAll("[data-boot-log] li")];
    const bar = screen.querySelector("[data-boot-bar]");
    const enter = screen.querySelector("[data-boot-enter]");
    screen.hidden = false;
    document.body.style.overflow = "hidden";
    let i = 0;
    let timer = null;
    const finishLog = () => {
      clearInterval(timer);
      lines.forEach((l) => l.classList.add("is-on"));
      bar.style.width = "100%";
      enter.classList.add("is-on");
    };
    const leave = () => {
      finishLog();
      try { sessionStorage.setItem("os.booted", "1"); } catch {}
      screen.hidden = true;
      document.body.style.overflow = "";
      removeEventListener("keydown", leave);
      blip(1320, 70);
      done();
    };
    if (reduced) finishLog();
    else {
      timer = setInterval(() => {
        if (i >= lines.length) return finishLog();
        lines[i].classList.add("is-on");
        bar.style.width = `${((i + 1) / lines.length) * 100}%`;
        blip(lines[i].hasAttribute("data-warn") ? 220 : 520 + i * 40, 25);
        i += 1;
      }, 170);
    }
    addEventListener("keydown", leave, { once: true });
    screen.addEventListener("click", leave, { once: true });
  }

  // ---- File names on the desktop type themselves in.
  function typeNames() {
    if (reduced) return;
    document.querySelectorAll("[data-type]").forEach((el, n) => {
      const text = el.textContent;
      el.textContent = "";
      let i = 0;
      setTimeout(() => {
        const t = setInterval(() => {
          el.textContent = text.slice(0, ++i);
          if (i >= text.length) clearInterval(t);
        }, 35);
      }, 150 + n * 90);
    });
  }

  // ---- Windows
  const windows = new Map();
  let z = 10;
  let cascade = 0;

  function front(win) {
    windows.forEach((w) => w.classList.remove("is-front"));
    win.classList.add("is-front");
    win.style.zIndex = ++z;
  }

  function place(win) {
    if (phone() || win.dataset.placed) return;
    const desk = document.querySelector("[data-desktop]").getBoundingClientRect();
    const w = win.offsetWidth;
    const left = Math.max(16, Math.min((innerWidth - w) / 2 + cascade * 28 - 60, innerWidth - w - 16));
    const top = Math.max(16, 70 + cascade * 28) + scrollY - desk.top - scrollY;
    win.style.left = `${left}px`;
    win.style.top = `${Math.max(16, top)}px`;
    win.dataset.placed = "1";
    cascade = (cascade + 1) % 6;
  }

  function open(id, { focus = true } = {}) {
    const win = windows.get(id);
    if (!win) return;
    const wasHidden = win.hidden;
    win.hidden = false;
    place(win);
    front(win);
    document.querySelectorAll(`[data-open="${id}"]`).forEach((a) => a.setAttribute("aria-expanded", "true"));
    if (wasHidden && !reduced) {
      win.classList.remove("is-opening");
      void win.offsetWidth;
      win.classList.add("is-opening");
    }
    if (wasHidden) blip(990, 35);
    if (focus) win.focus({ preventScroll: true });
    if (location.hash !== `#${id}`) history.replaceState(null, "", `#${id}`);
    if (id === "terminal") win.querySelector("input")?.focus();
  }

  function close(win) {
    win.hidden = true;
    win.classList.remove("is-front");
    document.querySelectorAll(`[data-open="${win.id}"]`).forEach((a) => {
      a.setAttribute("aria-expanded", "false");
    });
    blip(330, 40);
    history.replaceState(null, "", location.pathname);
    const opener = document.querySelector(`.volume[data-open="${win.id}"]`);
    opener?.focus({ preventScroll: true });
  }

  function drag(win) {
    const bar = win.querySelector("[data-drag]");
    bar?.addEventListener("pointerdown", (e) => {
      if (phone() || e.target.closest("[data-close]")) return;
      front(win);
      const sx = e.clientX - win.offsetLeft;
      const sy = e.clientY - win.offsetTop;
      win.classList.add("is-dragging");
      bar.setPointerCapture(e.pointerId);
      const move = (ev) => {
        win.style.left = `${Math.max(0, Math.min(ev.clientX - sx, innerWidth - 80))}px`;
        win.style.top = `${Math.max(0, ev.clientY - sy)}px`;
      };
      const up = () => {
        win.classList.remove("is-dragging");
        bar.removeEventListener("pointermove", move);
        bar.removeEventListener("pointerup", up);
      };
      bar.addEventListener("pointermove", move);
      bar.addEventListener("pointerup", up);
    });
  }

  function windowsUp() {
    document.querySelectorAll("[data-window]").forEach((win) => {
      windows.set(win.id, win);
      win.hidden = true;
      win.tabIndex = -1;
      win.setAttribute("role", "dialog");
      win.addEventListener("pointerdown", () => front(win));
      win.querySelector("[data-close]")?.addEventListener("click", (e) => {
        e.preventDefault();
        close(win);
      });
      drag(win);
    });
    document.querySelectorAll("[data-open]").forEach((a) => {
      if (!windows.has(a.dataset.open)) return;
      a.setAttribute("aria-expanded", "false");
      a.addEventListener("click", (e) => {
        if (!document.querySelector("[data-desktop]")) return;
        e.preventDefault();
        open(a.dataset.open);
      });
    });
    addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      const top = [...windows.values()].filter((w) => !w.hidden).sort((a, b) => b.style.zIndex - a.style.zIndex)[0];
      if (top) close(top);
    });
    const id = location.hash.slice(1);
    if (windows.has(id)) open(id, { focus: false });
  }

  // ---- Terminal
  function terminal() {
    const term = document.querySelector("[data-terminal]");
    if (!term) return;
    const say = (text, cls) => {
      const p = document.createElement("p");
      p.textContent = text;
      if (cls) p.className = cls;
      line.before(p);
      term.scrollTop = term.scrollHeight;
    };
    const line = document.createElement("form");
    line.className = "terminal__line";
    line.innerHTML = '<label for="term-in">$</label><input id="term-in" autocomplete="off" spellcheck="false" aria-label="Terminal command">';
    term.append(line);
    const input = line.querySelector("input");
    const files = [...windows.keys()].filter((k) => k !== "terminal");
    const book = document.querySelector(".menubar__book")?.href;
    const commands = {
      help: () => "help, ls, open <name>, whoami, book, date, clear",
      ls: () => [...document.querySelectorAll(".volume__file")].map((f) => f.textContent || f.dataset.file).join("  "),
      whoami: () => "Adriano Dias. Freelance web developer, Brussels. Drupal, Laravel, static sites.",
      date: () => new Intl.DateTimeFormat("en-GB", { dateStyle: "full", timeStyle: "short", timeZone: "Europe/Brussels" }).format(new Date()),
      book: () => { if (book) open_(book); return "Opening the booking page…"; },
      clear: () => { term.querySelectorAll("p").forEach((p) => p.remove()); return null; },
      sudo: () => "Nice try.",
      rm: () => "Everything here is in git. Nothing is lost.",
    };
    const open_ = (url) => window.open(url, "_blank", "noopener");
    line.addEventListener("submit", (e) => {
      e.preventDefault();
      const raw = input.value.trim();
      input.value = "";
      if (!raw) return;
      say(`$ ${raw}`, "is-cmd");
      const [cmd, arg = ""] = raw.split(/\s+/, 2);
      if (cmd === "open" || cmd === "cat") {
        const name = arg.replace(/\..*$/, "");
        if (files.includes(name)) { say(`Opening ${arg}…`); open(name); }
        else say(`open: no such file: ${arg || "(nothing)"}. Try ls.`);
        return;
      }
      const out = commands[cmd] ? commands[cmd]() : `${cmd}: command not found. Try help.`;
      if (out) say(out);
      blip(740, 20);
    });
  }

  function start() {
    root.classList.add("os");
    clock();
    switches();
    if (!document.querySelector("[data-desktop]")) return;
    windowsUp();
    terminal();
    boot(typeNames);
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})();
