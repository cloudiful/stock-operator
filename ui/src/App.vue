<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ConnectionPanel from '@/components/ConnectionPanel.vue'
import HistoryPanel from '@/components/HistoryPanel.vue'
import { setLocale, getCurrentLocale, type AppLocale } from '@/i18n'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { isTauri } from '@/utils/invoke'

const { t } = useI18n()
const { currentTheme, setTheme } = useTheme()

const activeTab = ref<'connection' | 'history'>('connection')
const globalMessage = ref('')
const globalKind = ref<'ok' | 'err' | 'info'>('info')
const showMessage = ref(false)
let messageTimer: number | null = null

const isTauriEnv = computed(() => isTauri())

function displayMessage(text: string, kind: 'ok' | 'err' | 'info' = 'info') {
  globalMessage.value = text
  globalKind.value = kind
  showMessage.value = true
  if (messageTimer) window.clearTimeout(messageTimer)
  messageTimer = window.setTimeout(() => (showMessage.value = false), 6000)
}

function onMessage(text: string, kind: 'ok' | 'err' | 'info') {
  displayMessage(text, kind)
}

const locale = ref<AppLocale>(getCurrentLocale())
function onLocaleChange(e: Event) {
  const val = (e.target as HTMLSelectElement).value as AppLocale
  locale.value = val
  setLocale(val)
}
function onThemeChange(e: Event) {
  const val = (e.target as HTMLSelectElement).value as ThemeMode
  setTheme(val)
}

onMounted(() => {
  locale.value = getCurrentLocale()
  if (!isTauriEnv.value) {
    displayMessage(t('banner.browserPreview'), 'info')
  }
})
</script>

<template>
  <div>
    <header class="topbar">
      <div class="brand">{{ t('common.brand') }}</div>
      <nav class="tabs" role="tablist">
        <button
          class="tab"
          :class="{ active: activeTab === 'connection' }"
          role="tab"
          :aria-selected="activeTab === 'connection'"
          @click="activeTab = 'connection'"
        >
          {{ t('common.tabs.connection') }}
        </button>
        <button
          class="tab"
          :class="{ active: activeTab === 'history' }"
          role="tab"
          :aria-selected="activeTab === 'history'"
          @click="activeTab = 'history'"
        >
          {{ t('common.tabs.history') }}
        </button>
      </nav>
      <div class="top-actions">
        <label class="selector">
          <span class="hint">{{ t('topbar.language') }}</span>
          <select :value="locale" @change="onLocaleChange" aria-label="language selector">
            <option value="zh-CN">{{ t('locale.zh-CN') }}</option>
            <option value="en">{{ t('locale.en') }}</option>
          </select>
        </label>
        <label class="selector">
          <span class="hint">{{ t('topbar.theme') }}</span>
          <select :value="currentTheme" @change="onThemeChange" aria-label="theme selector">
            <option value="system">{{ t('theme.system') }}</option>
            <option value="light">{{ t('theme.light') }}</option>
            <option value="dark">{{ t('theme.dark') }}</option>
          </select>
        </label>
        <span class="badge">{{ t('topbar.instance') }} —</span>
      </div>
    </header>

    <main class="container">
      <div v-if="!isTauriEnv" class="banner">{{ t('banner.fallback') }}</div>
      <div v-if="showMessage" class="message" :class="globalKind" role="status">{{ globalMessage }}</div>

      <section v-show="activeTab === 'connection'" role="tabpanel">
        <ConnectionPanel @message="onMessage" />
      </section>
      <section v-show="activeTab === 'history'" role="tabpanel">
        <HistoryPanel @message="onMessage" />
      </section>
    </main>
  </div>
</template>

<style scoped>
.selector {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 12px;
}
.selector select {
  padding: 4px 8px;
  font-size: 12px;
}
</style>
