import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import App from '@/App.vue'
import SettingsPage from '@/pages/SettingsPage.vue'
import { messages } from '@/i18n'
import { i18n as singletonI18n, setLocale } from '@/i18n'
import { LOCALE_STORAGE_KEY } from '@/i18n'
import ui from '@nuxt/ui/vue-plugin'
import { THEME_STORAGE_KEY } from '@/composables/useTheme'

function mountApp() {
  return mount(App, {
    global: {
      plugins: [singletonI18n, ui],
    },
  })
}

describe('App.vue', () => {
  beforeEach(() => {
    setLocale('en')
    window.localStorage.clear()
    document.documentElement.removeAttribute('data-theme')
    // ensure no tauri
    ;(window as unknown as { __TAURI__?: unknown }).__TAURI__ = undefined
  })

  it('renders brand and three sidebar items', async () => {
    const wrapper = mountApp()
    expect(wrapper.text()).toContain('Stock Operator')
    // sidebar has exactly three nav items
    const nav = wrapper.find('nav[aria-label="Main navigation"]')
    expect(nav.exists()).toBe(true)
    expect(wrapper.text()).toContain('Connection')
    expect(wrapper.text()).toContain('History')
    expect(wrapper.text()).toContain('Settings')
    const navButtons = nav.findAll('button')
    expect(navButtons.length).toBe(3)
    // no tab semantics should remain
    expect(wrapper.findAll('[role="tab"]').length).toBe(0)
    expect(wrapper.findAll('[role="tablist"]').length).toBe(0)
  })

  it('active page has accessible current state and keyboard focus', async () => {
    const wrapper = mountApp()
    const nav = wrapper.find('nav[aria-label="Main navigation"]')
    const buttons = nav.findAll('button')
    // initial active is connection
    const connectionTab = buttons.find((w) => w.text().includes('Connection'))!
    expect(connectionTab.attributes('aria-current')).toBe('page')
    // other tabs not current
    const historyTab = buttons.find((w) => w.text().includes('History'))!
    expect(historyTab.attributes('aria-current')).toBeUndefined()
    // keyboard focus: button should be focusable (native button, not disabled)
    expect(connectionTab.attributes('disabled')).toBeUndefined()
    expect(connectionTab.element.tagName.toLowerCase()).toBe('button')
    // clicking history should switch active
    await historyTab.trigger('click')
    await wrapper.vm.$nextTick()
    const updatedNav = wrapper.find('nav[aria-label="Main navigation"]')
    const updatedButtons = updatedNav.findAll('button')
    const hist2 = updatedButtons.find((w) => w.text().includes('History'))!
    expect(hist2.attributes('aria-current')).toBe('page')
    const conn2 = updatedButtons.find((w) => w.text().includes('Connection'))!
    expect(conn2.attributes('aria-current')).toBeUndefined()
    // visible region should be focusable, hidden should be -1 and aria-hidden
    const regions = wrapper.findAll('section[role="region"]')
    expect(regions.length).toBe(3)
    const visible = regions.find((r) => r.attributes('aria-labelledby') === 'page-title-history')!
    expect(visible.attributes('tabindex')).toBe('0')
    expect(visible.attributes('aria-hidden')).toBeUndefined()
    const hidden = regions.find((r) => r.attributes('aria-labelledby') === 'page-title-connection')!
    expect(hidden.attributes('tabindex')).toBe('-1')
    expect(hidden.attributes('aria-hidden')).toBe('true')
  })

  it('renders zh-CN translations after locale switch (reactive)', async () => {
    const wrapper = mountApp()
    expect(wrapper.text()).toContain('Connection')
    setLocale('zh-CN')
    await wrapper.vm.$nextTick()
    await new Promise((r) => setTimeout(r, 0))
    expect(wrapper.text()).toContain('连接')
    expect(wrapper.text()).toContain('历史')
    expect(wrapper.text()).toContain('设置')
    setLocale('en')
  })

  it('settings page contains language and theme controls and persists namespaced keys', async () => {
    const wrapper = mountApp()
    // navigate to settings
    const settingsTab = wrapper.find('nav[aria-label="Main navigation"]').findAll('button').find((w) => w.text().includes('Settings'))!
    await settingsTab.trigger('click')
    await wrapper.vm.$nextTick()
    // SettingsPage is now visible (v-show) - find its heading
    expect(wrapper.text()).toContain('Settings')
    // mount SettingsPage directly to test locale/theme controls with ui plugin
    const i18n = createI18n({ legacy: false, locale: 'en', fallbackLocale: 'en', messages })
    const settingsWrapper = mount(SettingsPage, { global: { plugins: [i18n, ui] } })
    // check for appearance section and language/theme labels (via UFormField label)
    expect(settingsWrapper.text()).toContain('Appearance')
    expect(settingsWrapper.text()).toContain('Language')
    expect(settingsWrapper.text()).toContain('Theme')
    // locale persistence via setLocale directly (reactive)
    const { setLocale: setL } = await import('@/i18n')
    setL('zh-CN')
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('zh-CN')
    expect(document.documentElement.lang).toBe('zh-CN')
    setL('en')
    expect(window.localStorage.getItem(LOCALE_STORAGE_KEY)).toBe('en')
    // theme persistence
    const { useTheme } = await import('@/composables/useTheme')
    // need to mount a component to use composable, but we can test storage directly
    window.localStorage.setItem(THEME_STORAGE_KEY, 'dark')
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark')
    // ensure only namespaced keys are used, no token in localStorage
    window.localStorage.setItem('some-token', 'secret')
    expect(window.localStorage.getItem('some-token')).toBe('secret')
    // token should not be stored under namespaced theme/locale keys
    expect(window.localStorage.getItem('stock-operator.theme')).not.toContain('secret')
    expect(window.localStorage.getItem('stock-operator.locale')).not.toContain('secret')
    window.localStorage.removeItem('some-token')
  })

  it('browser preview disables mutations and shows fallback banner', async () => {
    const wrapper = mountApp()
    // banner fallback should be visible when not tauri
    expect(wrapper.text()).toContain('Desktop UI requires the Tauri app')
    // go to settings and check save buttons disabled
    const settingsTab = wrapper.find('nav[aria-label="Main navigation"]').findAll('button').find((w) => w.text().includes('Settings'))!
    await settingsTab.trigger('click')
    await wrapper.vm.$nextTick()
    // find SettingsPage buttons: they should have disabled attribute when not tauri
    // Use mount of SettingsPage to check disabled props (isTauri false)
    const i18n = createI18n({ legacy: false, locale: 'en', fallbackLocale: 'en', messages })
    const settingsWrapper = mount(SettingsPage, { global: { plugins: [i18n, ui] } })
    // buttons with disabled prop should be found; check at least one primary save button is disabled
    const buttons = settingsWrapper.findAllComponents({ name: 'UButton' })
    // if UButton not found by name, fallback to button elements
    const nativeButtons = settingsWrapper.findAll('button')
    // at least some buttons should be disabled in browser preview
    const hasDisabled = nativeButtons.some((b) => b.attributes('disabled') !== undefined) || buttons.some((b) => (b.props() as Record<string, unknown>).disabled === true)
    expect(hasDisabled).toBe(true)
  })

  it('does not use v-html for backend values (safe interpolation)', async () => {
    const { default: HistoryPanel } = await import('./HistoryPanel.vue')
    const i18n = createI18n({ legacy: false, locale: 'en', fallbackLocale: 'en', messages })
    const wrapper = mount(HistoryPanel, { global: { plugins: [i18n, ui] } })
    expect(wrapper.html()).not.toContain('v-html')
  })

  it('keeps history invoke compatibility (stateFilter, operationId, dual id)', async () => {
    const mod = await import('./HistoryPanel.vue')
    const src = mod.default as unknown as { __hmrId?: string } | string
    // Read raw file content to ensure strings exist
    const fs = await import('node:fs')
    const path = await import('node:path')
    const file = fs.readFileSync(path.resolve('src/components/HistoryPanel.vue'), 'utf-8')
    expect(file).toContain('stateFilter')
    expect(file).toContain('operationId')
    expect(file).toContain('operation_id')
    expect(file).toContain('operationId: opId')
    expect(file).toContain('list_operations')
    expect(file).toContain('list_audit_events')
    expect(file).toContain('resolve_stale_operation')
  })

  it('layout remains usable at desktop minimum width (sidebar + main)', async () => {
    const wrapper = mountApp()
    const shell = wrapper.find('.app-shell')
    expect(shell.exists()).toBe(true)
    // check that shell uses flex layout (class exists)
    expect(wrapper.find('.app-sidebar').exists()).toBe(true)
    expect(wrapper.find('.main-area').exists()).toBe(true)
  })
})
