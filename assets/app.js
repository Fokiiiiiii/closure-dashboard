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
