// CSP-compatible external bootstrap: apply theme before first Vue paint to avoid white flash.
// No inline script used; this file is loaded via <script src> with script-src 'self'.
(function () {
  try {
    var STORAGE_KEY = 'stock-operator.theme'
    var mode = null
    try {
      mode = localStorage.getItem(STORAGE_KEY)
    } catch (_) {}
    if (mode !== 'light' && mode !== 'dark' && mode !== 'system') mode = 'system'
    var resolved = mode
    if (mode === 'system') {
      try {
        resolved = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
      } catch (_) {
        resolved = 'light'
      }
    }
    var root = document.documentElement
    root.setAttribute('data-theme', resolved)
    root.style.colorScheme = resolved
    // Ensure background applied immediately for flash avoidance
    // CSS variables will handle actual colors; this just marks readiness.
    root.classList.add('theme-ready')
  } catch (_) {}
})()
