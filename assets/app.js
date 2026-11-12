(() => {
  // The zone of the browser, so that a time typed for a scheduled post means the writer's own.
  for (const input of document.querySelectorAll('input[name="tz"]')) {
    input.value = String(new Date().getTimezoneOffset());
  }
})();

(() => {
  const form = document.querySelector("form[data-preview]");
  const target = document.getElementById("preview");
  if (!form || !target) return;
  let timer;
  // The call of an earlier keystroke is given up when a newer one starts, so an old answer never
  // lands after a newer one.
  let inFlight;
  const refresh = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      if (inFlight) inFlight.abort();
      const mine = new AbortController();
      inFlight = mine;
      try {
        const response = await fetch(form.dataset.preview, {
          method: "POST",
          body: new URLSearchParams(new FormData(form)),
          headers: { "content-type": "application/x-www-form-urlencoded" },
          signal: mine.signal,
        });
        if (response.ok && inFlight === mine) target.innerHTML = await response.text();
      } catch {
        // The preview is a courtesy: the page still works without it.
      }
    }, 400);
  };
  form.addEventListener("input", refresh);
  form.addEventListener("change", refresh);
})();

(() => {
  if (typeof Intl.supportedValuesOf !== "function") return;
  let timeZones;
  try {
    timeZones = Intl.supportedValuesOf("timeZone");
  } catch {
    return;
  }
  for (const list of document.querySelectorAll("datalist[data-time-zone-list]")) {
    const known = new Set(Array.from(list.options, (option) => option.value));
    for (const timeZone of timeZones) {
      if (known.has(timeZone)) continue;
      const option = document.createElement("option");
      option.value = timeZone;
      list.append(option);
    }
  }
})();


(() => {
  const results = document.getElementById("ticket-analytics-results");
  const loading = document.querySelector("[data-analytics-loading]");
  if (!results) return;

  const period = new URLSearchParams(location.search).get("days") || "30";
  const storageKey = `closure-ticket-analytics:${location.pathname}:${period}`;
  const setBusy = (link, busy) => {
    if (!link) return;
    if (busy) {
      link.setAttribute("aria-disabled", "true");
      link.style.pointerEvents = "none";
    } else {
      link.removeAttribute("aria-disabled");
      link.style.pointerEvents = "";
    }
  };
  const run = async () => {
    let completedPages = 0;
    try {
      completedPages = Number(sessionStorage.getItem(storageKey) || "0");
    } catch {
      // Continue this page's bounded scan even when storage is unavailable.
    }
    if (!Number.isFinite(completedPages) || completedPages >= 20) return;

    if (loading) loading.hidden = false;
    for (; completedPages < 20; completedPages += 1) {
      const link = results.querySelector("a[data-analytics-continue]");
      if (!link) {
        try {
          sessionStorage.removeItem(storageKey);
        } catch {
          // The visible result remains complete even when storage is unavailable.
        }
        if (loading) loading.hidden = true;
        return;
      }
      const url = new URL(link.href, location.href);
      if (url.origin !== location.origin) break;
      setBusy(link, true);
      try {
        const response = await fetch(url.toString(), { credentials: "same-origin" });
        if (!response.ok) break;
        const document = new DOMParser().parseFromString(await response.text(), "text/html");
        const nextResults = document.getElementById("ticket-analytics-results");
        if (!nextResults) break;
        results.innerHTML = nextResults.innerHTML;
        try {
          sessionStorage.setItem(storageKey, String(completedPages + 1));
        } catch {
          // The scan remains bounded to 20 pages for this visit.
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      } catch {
        break;
      }
    }
    setBusy(results.querySelector("a[data-analytics-continue]"), false);
    if (loading) loading.hidden = true;
  };
  void run();
})();

(() => {
  const timers = new WeakMap();
  const schedule = (form, delay) => {
    const previous = timers.get(form);
    if (previous) clearTimeout(previous);
    timers.set(form, setTimeout(() => form.requestSubmit(), delay));
  };

  document.addEventListener("change", (event) => {
    if (!(event.target instanceof Element)) return;
    const form = event.target.closest("form[data-transcript-autosubmit]");
    if (!form || !event.target.matches("select, input[type=checkbox]")) return;
    schedule(form, 0);
  });

  document.addEventListener("input", (event) => {
    if (!(event.target instanceof Element)) return;
    const form = event.target.closest("form[data-transcript-autosubmit]");
    if (!form || !event.target.matches('input[type="search"], input[type="text"]')) return;
    schedule(form, 300);
  });
})();
