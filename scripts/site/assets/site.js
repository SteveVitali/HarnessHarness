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
