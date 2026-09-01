import { createI18n } from 'vue-i18n'
import { en } from './locales/en'
import { zhCN } from './locales/zh-CN'

export const LOCALE_STORAGE_KEY = 'stock-operator.locale'
export const DEFAULT_LOCALE = 'en' as const
export const SUPPORTED_LOCALES = ['zh-CN', 'en'] as const
export type AppLocale = (typeof SUPPORTED_LOCALES)[number]

export const messages = {
  'zh-CN': zhCN,
  en,
} as const

function normalizeLocale(value?: string | null): AppLocale | null {
  if (!value) return null
  const lowered = value.toLowerCase()
  if (lowered === 'zh-cn' || lowered.startsWith('zh')) return 'zh-CN'
  if (lowered === 'en' || lowered.startsWith('en')) return 'en'
  return null
}

export function detectBrowserLocale(): AppLocale {
  if (typeof navigator === 'undefined') return DEFAULT_LOCALE
  const candidates = [...(navigator.languages ?? []), navigator.language]
  for (const candidate of candidates) {
    const loc = normalizeLocale(candidate)
    if (loc) return loc
  }
  return DEFAULT_LOCALE
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

export function getStoredLocale(): AppLocale | null {
  try {
    const s = safeStorage()
    if (!s) return null
    return normalizeLocale(s.getItem(LOCALE_STORAGE_KEY))
  } catch {
    return null
  }
}

export function resolveInitialLocale(): AppLocale {
  return getStoredLocale() ?? detectBrowserLocale() ?? DEFAULT_LOCALE
}

export const i18n = createI18n({
  legacy: false,
  locale: resolveInitialLocale(),
  fallbackLocale: DEFAULT_LOCALE,
  messages,
  globalInjection: true,
})

export function setLocale(locale: AppLocale): void {
  i18n.global.locale.value = locale
  try {
    const s = safeStorage()
    if (s) s.setItem(LOCALE_STORAGE_KEY, locale)
  } catch {}
  if (typeof document !== 'undefined') document.documentElement.lang = locale === 'zh-CN' ? 'zh-CN' : 'en'
}

export function getCurrentLocale(): AppLocale {
  return i18n.global.locale.value as AppLocale
}

// Initialize document lang
if (typeof document !== 'undefined') {
  const cur = getCurrentLocale()
  document.documentElement.lang = cur === 'zh-CN' ? 'zh-CN' : 'en'
}
