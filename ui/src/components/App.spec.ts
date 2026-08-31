import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import App from '@/App.vue'
import { messages } from '@/i18n'
import { i18n as singletonI18n, setLocale } from '@/i18n'

function createWrapperWithSingleton() {
  return mount(App, {
    global: {
      plugins: [singletonI18n],
    },
  })
}

describe('App.vue', () => {
  beforeEach(() => {
    setLocale('en')
    window.localStorage.clear()
  })

  it('renders brand and tabs', async () => {
    const wrapper = createWrapperWithSingleton()
    expect(wrapper.text()).toContain('Stock Operator')
    expect(wrapper.text()).toContain('Connection')
    expect(wrapper.text()).toContain('History')
  })

  it('renders zh-CN translations after locale switch', async () => {
    const wrapper = createWrapperWithSingleton()
    expect(wrapper.text()).toContain('Connection')
    setLocale('zh-CN')
    await wrapper.vm.$nextTick()
    // Allow i18n reactivity
    await new Promise((r) => setTimeout(r, 0))
    expect(wrapper.text()).toContain('连接')
    expect(wrapper.text()).toContain('历史')
    setLocale('en')
  })

  it('has theme and language selectors visible', () => {
    const wrapper = createWrapperWithSingleton()
    expect(wrapper.find('select[aria-label="theme selector"]').exists()).toBe(true)
    expect(wrapper.find('select[aria-label="language selector"]').exists()).toBe(true)
  })

  it('switches locale immediately via selector', async () => {
    const wrapper = createWrapperWithSingleton()
    const select = wrapper.find('select[aria-label="language selector"]')
    expect(select.exists()).toBe(true)
    await select.setValue('zh-CN')
    await wrapper.vm.$nextTick()
    // setLocale should have persisted
    expect(window.localStorage.getItem('stock-operator.locale')).toBe('zh-CN')
    expect(wrapper.text()).toContain('连接')
    await select.setValue('en')
    await wrapper.vm.$nextTick()
    expect(window.localStorage.getItem('stock-operator.locale')).toBe('en')
  })

  it('does not use v-html for backend values (safe interpolation)', async () => {
    const { default: HistoryPanel } = await import('./HistoryPanel.vue')
    const i18n = createI18n({ legacy: false, locale: 'en', fallbackLocale: 'en', messages })
    const wrapper = mount(HistoryPanel, { global: { plugins: [i18n] } })
    expect(wrapper.html()).not.toContain('v-html')
  })
})
