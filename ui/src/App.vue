<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import AppSidebar, { type AppPage } from '@/components/AppSidebar.vue'
import ConnectionPage from '@/pages/ConnectionPage.vue'
import HistoryPage from '@/pages/HistoryPage.vue'
import SettingsPage from '@/pages/SettingsPage.vue'
import { isTauri } from '@/utils/invoke'
import { ensureSingleInstanceListener } from '@/composables/useRuntime'

const { t } = useI18n()

const activePage = ref<AppPage>('connection')
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

onMounted(() => {
  if (!isTauriEnv.value) {
    displayMessage(t('banner.browserPreview'), 'info')
  }
  ensureSingleInstanceListener()
})
</script>

<template>
  <UApp>
    <div class="app-shell">
      <AppSidebar :active-page="activePage" @update:active-page="activePage = $event" />
      <div class="main-area">
        <main class="container">
          <div v-if="!isTauriEnv" class="banner" role="note">{{ t('banner.fallback') }}</div>
          <div v-if="showMessage" class="message" :class="globalKind" role="status" aria-live="polite">{{ globalMessage }}</div>

          <section v-show="activePage === 'connection'" role="region" aria-labelledby="page-title-connection" :tabindex="activePage === 'connection' ? 0 : -1" :aria-hidden="activePage === 'connection' ? undefined : 'true'">
            <ConnectionPage @message="onMessage" />
          </section>
          <section v-show="activePage === 'history'" role="region" aria-labelledby="page-title-history" :tabindex="activePage === 'history' ? 0 : -1" :aria-hidden="activePage === 'history' ? undefined : 'true'">
            <HistoryPage @message="onMessage" />
          </section>
          <section v-show="activePage === 'settings'" role="region" aria-labelledby="page-title-settings" :tabindex="activePage === 'settings' ? 0 : -1" :aria-hidden="activePage === 'settings' ? undefined : 'true'">
            <SettingsPage @message="onMessage" />
          </section>
        </main>
      </div>
    </div>
  </UApp>
</template>

<style scoped>
.app-shell {
  display: flex;
  min-height: 100vh;
  background: var(--bg);
}
.main-area {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.container {
  max-width: 1120px;
  margin: 0 auto;
  padding: 16px;
  width: 100%;
  flex: 1;
}
.banner {
  padding: 10px 12px;
  border: 1px solid #fef3c7;
  background: #fffbeb;
  color: #92400e;
  border-radius: 8px;
  margin-bottom: 12px;
  font-size: 13px;
}
[data-theme="dark"] .banner {
  background: #422006;
  border-color: #854d0e;
  color: #fef3c7;
}
.message {
  padding: 10px 12px;
  border-radius: 8px;
  margin-bottom: 12px;
  font-size: 13px;
}
.message.ok {
  background: #f0fdf4;
  border: 1px solid #bbf7d0;
  color: #166534;
}
[data-theme="dark"] .message.ok {
  background: #052e16;
  border-color: #14532d;
  color: #bbf7d0;
}
.message.err {
  background: #fef2f2;
  border: 1px solid #fecaca;
  color: #991b1b;
}
[data-theme="dark"] .message.err {
  background: #450a0a;
  border-color: #7f1d1d;
  color: #fecaca;
}
.message.info {
  background: #eff6ff;
  border: 1px solid #bfdbfe;
  color: #1e40af;
}
[data-theme="dark"] .message.info {
  background: #0c1a3a;
  border-color: #1e3a8a;
  color: #bfdbfe;
}

@media (max-width: 960px) {
  .app-shell {
    flex-direction: column;
  }
}
</style>
