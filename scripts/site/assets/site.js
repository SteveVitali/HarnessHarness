/* site.js — theme toggle only. The before-paint theme resolution is the
   inline hashed <script> in each page head (design/39 F13); this file
   wires the <button aria-pressed> toggle. No other behaviour is required:
   every value is pre-rendered (REQ-XF-109). */
(function () {
  var b = document.querySelector('.theme-toggle');
  if (!b) { return; }
  function cur() {
    var t = document.documentElement.dataset.theme;
    if (t === 'light' || t === 'dark') { return t; }
    return (window.matchMedia && matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
  }
  function paint() { b.setAttribute('aria-pressed', cur() === 'dark' ? 'true' : 'false'); }
  b.addEventListener('click', function () {
    var t = cur() === 'dark' ? 'light' : 'dark';
    try { window.localStorage.setItem('hh-theme', t); } catch (e) { /* private mode: session-only */ }
    document.documentElement.dataset.theme = t;
    paint();
  });
  try { window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', paint); } catch (e) { /* older engine */ }
  paint();
}());

/* view toggles (R6.20) — chip groups switch pre-rendered variants by
   toggling `hidden` only; every value was rendered by the generator and
   nothing is computed here (REQ-XF-115). */
(function () {
  var groups = document.querySelectorAll('[data-toggle-group]');
  if (!groups.length) { return; }
  groups.forEach(function (g) {
    var name = g.getAttribute('data-toggle-group');
    var chips = g.querySelectorAll('.chip[data-toggle-value]');
    chips.forEach(function (chip) {
      chip.addEventListener('click', function () {
        var v = chip.getAttribute('data-toggle-value');
        chips.forEach(function (c) {
          c.setAttribute('aria-pressed', c === chip ? 'true' : 'false');
        });
        var root = g.closest('[data-toggle-root]') || document;
        root.querySelectorAll('[data-variant-of="' + name + '"]').forEach(function (el) {
          el.hidden = el.getAttribute('data-variant') !== v;
        });
      });
    });
  });
}());
