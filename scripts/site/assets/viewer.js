/* viewer.js — the dormant transcript viewer (REQ-XF-132, C-07).
   The site's single declared REQ-XF-009 runtime exception: it fetches
   ONLY on an explicit click on a .viewer-load button, ONLY the URL the
   generator stamped on the button (the declared dataset origin at the
   indexed revision — connect-src on the page names that origin set),
   verifies sha256 in the browser BEFORE rendering, renders text only
   (textContent — never HTML), strips ANSI control sequences, and caps
   the fetch. A sha256 mismatch renders refused{sha256_mismatch};
   any fetch/shape failure renders refused{fetch} — never silent.
   While the receipt index says private no button is emitted at all —
   withheld{dataset_private} carries the sha256 and no link. */
(function () {
  "use strict";
  var CAP = 5242880; // 5 MB — condensed transcripts only (design/39 F11)
  var ANSI = /\[[0-9;?]*[ -\/]*[@-~]/g;

  function hex(buf) {
    var b = new Uint8Array(buf), s = "";
    for (var i = 0; i < b.length; i++) {
      s += ("0" + b[i].toString(16)).slice(-2);
    }
    return s;
  }

  function fail(out, token) {
    out.textContent = token;
  }

  function load(btn) {
    var out = btn.parentElement.querySelector("[data-viewer-out]");
    var url = btn.getAttribute("data-url");
    var want = (btn.getAttribute("data-sha256") || "").toLowerCase();
    var bytes = parseInt(btn.getAttribute("data-bytes") || "0", 10);
    btn.disabled = true;
    out.textContent = "loading…";
    if (bytes > CAP) { fail(out, "refused{over_size_cap}"); return; }
    fetch(url, {mode: "cors", credentials: "omit", cache: "default"})
      .then(function (r) {
        if (!r.ok) { throw new Error("http " + r.status); }
        var len = r.headers.get("content-length");
        if (len && parseInt(len, 10) > CAP) {
          throw new Error("over_size_cap");
        }
        return r.arrayBuffer();
      })
      .then(function (buf) {
        if (buf.byteLength > CAP) { throw new Error("over_size_cap"); }
        return crypto.subtle.digest("SHA-256", buf).then(function (sum) {
          if (hex(sum) !== want) {
            fail(out, "refused{sha256_mismatch}");
            return;
          }
          var text = new TextDecoder("utf-8").decode(buf)
            .replace(ANSI, "");
          if (text.length > CAP) { text = text.slice(0, CAP); }
          out.textContent = text; // text only — never HTML
        });
      })
      .catch(function (e) {
        fail(out, e && e.message === "over_size_cap"
          ? "refused{over_size_cap}" : "refused{fetch}");
      })
      .finally(function () { btn.disabled = false; });
  }

  // Fetch exists only inside this click registration — no load of any
  // kind happens at parse or idle.
  document.querySelectorAll("button.viewer-load[data-url]")
    .forEach(function (b) {
      b.addEventListener("click", function () { load(b); });
    });
}());
