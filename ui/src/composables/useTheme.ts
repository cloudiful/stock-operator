import { ref, watch, onMounted, onBeforeUnmount } from 'vue'

export type ThemeMode = 'system' | 'light' | 'dark'
export const THEME_STORAGE_KEY = 'stock-operator.theme'
const VALID_MODES: ThemeMode[] = ['system', 'light', 'dark']

function normalizeMode(value: string | null): ThemeMode {
  if (value === 'light' || value === 'dark' || value === 'system') return value
  return 'system'
}

function safeStorage(): Storage | null {
  try {
    if (typeof window !== 'undefined' && window.localStorage) return window.localStorage
    if (typeof globalThis !== 'undefined' && (globalThis as unknown as { localStorage?: Storage }).localStorage) {
      return (globalThis as unknown as { localStorage: Storage }).localStorage
    }
  } catch {}
  return null
}

function getStoredTheme(): ThemeMode {
  try {
    const s = safeStorage()
    if (!s) return 'system'
    return normalizeMode(s.getItem(THEME_STORAGE_KEY))
  } catch {
    return 'system'
  }
}

function resolveTheme(mode: ThemeMode): 'light' | 'dark' {
  if (mode === 'light' || mode === 'dark') return mode
  try {
    if (typeof window !== 'undefined' && window.matchMedia) {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
    }
  } catch {}
  return 'light'
}

function applyTheme(mode: ThemeMode): void {
  if (typeof document === 'undefined') return
  const resolved = resolveTheme(mode)
  const root = document.documentElement
  root.setAttribute('data-theme', resolved)
  root.style.colorScheme = resolved
  // Also keep attribute for mode itself for debugging
  root.setAttribute('data-theme-mode', mode)
}

const currentTheme = ref<ThemeMode>(getStoredTheme())

export function useTheme() {
  let media: MediaQueryList | null = null
  let handler: ((e: MediaQueryListEvent) => void) | null = null

  const setTheme = (mode: ThemeMode) => {
    const normalized = normalizeMode(mode)
    currentTheme.value = normalized
    try {
      const s = safeStorage()
      if (s) s.setItem(THEME_STORAGE_KEY, normalized)
    } catch {}
    applyTheme(normalized)
  }

  const init = () => {
    applyTheme(currentTheme.value)
  }

  watch(currentTheme, (mode) => {
    applyTheme(mode)
  })

  onMounted(() => {
    applyTheme(currentTheme.value)
    try {
      media = window.matchMedia('(prefers-color-scheme: dark)')
      handler = () => {
        if (currentTheme.value === 'system') applyTheme('system')
      }
      // Modern browsers
      if (media.addEventListener) media.addEventListener('change', handler as EventListener)
      else (media as unknown as { addListener: (fn: typeof handler) => void }).addListener(handler)
    } catch {}
  })

  onBeforeUnmount(() => {
    try {
      if (media && handler) {
        if (media.removeEventListener) media.removeEventListener('change', handler as EventListener)
        else (media as unknown as { removeListener: (fn: typeof handler) => void }).removeListener(handler)
      }
    } catch {}
  })

  return {
    currentTheme,
    setTheme,
    init,
    resolveTheme,
    THEME_STORAGE_KEY,
  }
}

// Eager apply for non-component usage (bootstrap fallback)
export function initThemeEarly(): void {
  applyTheme(getStoredTheme())
}
