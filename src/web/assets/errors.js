// Places the HTML fragment of a failed request next to the control that caused
// it. The server answers a refusal with 422, a conflict with 409, and so on,
// with a body `<div id="err-CONTROL" role="alert">...</div>`. Datastar 1.0.5
// ignores the body of any response that is not 200 and aborts the request as
// soon as it sees the status, so the body cannot be read from its response.
// This wraps fetch: it gives the request its own abort signal, which it does
// not trigger once the status is 4xx/5xx, reads the body, and replaces the
// element with the same id. The page needs an element with that id in place;
// without one the fragment is dropped.
(() => {
  const nativeFetch = window.fetch;

  const isHtml = (r) => (r.headers.get("Content-Type") || "").includes("text/html");

  const place = (html) => {
    const doc = new DOMParser().parseFromString(html, "text/html");
    const fragment = doc.body.firstElementChild;
    if (!fragment || !fragment.id.startsWith("err-")) return;
    const target = document.getElementById(fragment.id);
    if (target) target.replaceWith(document.importNode(fragment, true));
  };

  window.fetch = async (input, init) => {
    const outer = init && init.signal;
    if (!outer) return nativeFetch(input, init);
    const own = new AbortController();
    let failed = false;
    const forward = () => {
      if (!failed) own.abort();
    };
    if (outer.aborted) forward();
    outer.addEventListener("abort", forward);
    const response = await nativeFetch(input, { ...init, signal: own.signal });
    if (response.status >= 400 && isHtml(response)) {
      failed = true;
      response
        .clone()
        .text()
        .then(place)
        .catch(() => {});
    }
    return response;
  };
})();
