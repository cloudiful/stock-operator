import { describe, it, expect, beforeEach, vi } from 'vitest'
import { detectBrowserLocale, resolveInitialLocale, getStoredLocale, LOCALE_STORAGE_KEY } from './index'

describe('i18n locale', () => {
  beforeEach(() => {
    window.localStorage.clear()
    vi.restoreAllMocks()
  })

  it('normalizes zh to zh-CN', () => {
    Object.defineProperty(window, 'navigator', {
      value: { language: 'zh-CN', languages: ['zh-CN'] },
      writable: true,
    })
    expect(detectBrowserLocale()).toBe('zh-CN')
  })

  it('normalizes en to en', () => {
    Object.defineProperty(window, 'navigator', {
      value: { language: 'en-US', languages: ['en-US'] },
      writable: true,
    })
    expect(detectBrowserLocale()).toBe('en')
  })

  it('fallbacks to en for unknown', () => {
    Object.defineProperty(window, 'navigator', {
      value: { language: 'fr-FR', languages: ['fr-FR'] },
      writable: true,
    })
    expect(detectBrowserLocale()).toBe('en')
  })

  it('prefers stored locale over browser', () => {
    window.localStorage.setItem(LOCALE_STORAGE_KEY, 'zh-CN')
    Object.defineProperty(window, 'navigator', {
      value: { language: 'en-US', languages: ['en-US'] },
      writable: true,
    })
    expect(resolveInitialLocale()).toBe('zh-CN')
    expect(getStoredLocale()).toBe('zh-CN')
  })

  it('uses browser when no stored', () => {
    Object.defineProperty(window, 'navigator', {
      value: { language: 'zh-TW', languages: ['zh-TW'] },
      writable: true,
    })
    expect(resolveInitialLocale()).toBe('zh-CN')
  })

  it('persists locale under namespaced key', async () => {
    const { setLocale, getCurrentLocale } = await import('./index')
    setLocale('zh-CN')
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('zh-CN')
    expect(getCurrentLocale()).toBe('zh-CN')
    setLocale('en')
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('en')
  })
})
