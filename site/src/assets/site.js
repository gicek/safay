// Safay website: the rotating word and the demo code in the hero, and the language choice. No dependencies, no tracking.
(() => {
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Language: the first visit to "/" follows the browser language; a choice in the switcher is remembered locally.
  const store = {
    get() { try { return localStorage.getItem("safay.lang"); } catch { return null; } },
    set(value) { try { localStorage.setItem("safay.lang", value); } catch { /* private mode: nothing to remember */ } },
  };
  const prefixes = { en: "/", "zh-Hans": "/zh-hans/", "zh-Hant": "/zh-hant/" };
  const preferred = () => {
    for (const tag of navigator.languages || [navigator.language || "en"]) {
      const lower = tag.toLowerCase();
      if (!lower.startsWith("zh")) { if (lower.startsWith("en")) return "en"; continue; }
      return /hant|-tw|-hk|-mo/.test(lower) ? "zh-Hant" : "zh-Hans";
    }
    return "en";
  };
  if (location.pathname === "/" && !store.get()) {
    const lang = preferred();
    if (lang !== "en") location.replace(prefixes[lang] + location.hash);
  }
  document.querySelectorAll(".lang-list a[data-lang]").forEach((link) => {
    link.addEventListener("click", () => store.set(link.dataset.lang));
  });
  document.addEventListener("click", (event) => {
    document.querySelectorAll(".lang-menu[open]").forEach((menu) => {
      if (!menu.contains(event.target)) menu.removeAttribute("open");
    });
  });
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") document.querySelectorAll(".lang-menu[open]").forEach((menu) => menu.removeAttribute("open"));
  });

  // Header hairline once the page scrolls.
  const header = document.querySelector(".site-header");
  if (header) {
    const update = () => header.classList.toggle("is-scrolled", scrollY > 4);
    addEventListener("scroll", update, { passive: true });
    update();
  }

  // Rotating word in the headline. Screen readers get the full sentence from the hidden copy instead.
  document.querySelectorAll(".rotator[data-words]").forEach((slot) => {
    const words = slot.dataset.words.split("|");
    if (words.length < 2 || reduceMotion) return;
    let index = 0;
    let current = slot.querySelector(".rotator-word");
    const widthOf = (element) => element.getBoundingClientRect().width;
    slot.style.width = `${widthOf(current)}px`;
    const swap = () => {
      if (document.hidden) return;
      index = (index + 1) % words.length;
      const next = document.createElement("span");
      next.className = "rotator-word is-entering";
      next.textContent = words[index];
      current.classList.add("is-leaving");
      const leaving = current;
      leaving.addEventListener("animationend", () => leaving.remove(), { once: true });
      // Fallback: animationend may never fire (background tab, animations off by the browser).
      setTimeout(() => leaving.remove(), 700);
      next.addEventListener("animationend", () => next.classList.remove("is-entering"), { once: true });
      slot.append(next);
      slot.style.width = `${widthOf(next)}px`;
      current = next;
    };
    // Fonts change word widths; measure again once they are in.
    document.fonts?.ready.then(() => { slot.style.width = `${widthOf(current)}px`; });
    addEventListener("resize", () => { slot.style.width = `${widthOf(current)}px`; }, { passive: true });
    setInterval(swap, 2600);
  });

  // Demo code in the hero: random digits on a real 30-second clock. Not a real account and not a real secret.
  const digits = document.querySelector("[data-code]");
  const ring = digits && digits.parentElement.querySelector(".ring");
  if (digits && ring) {
    const fresh = () => {
      const n = String(Math.floor(Math.random() * 1e6)).padStart(6, "0");
      return `${n.slice(0, 3)} ${n.slice(3)}`;
    };
    let step = Math.floor(Date.now() / 30000);
    digits.textContent = fresh();
    const tick = () => {
      const now = Date.now();
      const current = Math.floor(now / 30000);
      ring.style.setProperty("--p", String(1 - (now % 30000) / 30000));
      if (current !== step) {
        step = current;
        digits.textContent = fresh();
        if (!reduceMotion) {
          digits.classList.remove("is-changing");
          void digits.offsetWidth;
          digits.classList.add("is-changing");
        }
      }
    };
    tick();
    setInterval(tick, reduceMotion ? 1000 : 250);
  }
})();
