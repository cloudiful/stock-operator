<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettings } from '@/composables/useSettings'
import { useRuntime } from '@/composables/useRuntime'
import { useTheme, type ThemeMode } from '@/composables/useTheme'
import { setLocale, getCurrentLocale, type AppLocale } from '@/i18n'
import { isTauri } from '@/utils/invoke'

const { t } = useI18n()
const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

function onMessage(text: string, kind: 'ok' | 'err' | 'info') {
  emit('message', text, kind)
}

const settings = useSettings(onMessage)
const runtime = useRuntime(onMessage)
const { currentTheme, setTheme } = useTheme()

const locale = ref<AppLocale>(getCurrentLocale())
const canMutate = isTauri()

const networkItems = computed(() => [
  { label: t('connection.operatorNetwork.loopback'), value: 'loopback' },
  { label: t('connection.operatorNetwork.privateOverlay'), value: 'private-overlay' },
])

const themeItems = computed(() => [
  { label: t('theme.system'), value: 'system' },
  { label: t('theme.light'), value: 'light' },
  { label: t('theme.dark'), value: 'dark' },
])

const localeItems = computed(() => [
  { label: t('locale.zh-CN'), value: 'zh-CN' },
  { label: t('locale.en'), value: 'en' },
])

function onLocaleChange(val: AppLocale) {
  locale.value = val
  setLocale(val)
}
function onThemeChange(val: ThemeMode) {
  setTheme(val)
}

async function handleSave() {
  const res = await settings.saveSettings()
  if (res.ok) {
    await Promise.all([settings.loadSettings(), runtime.loadStatus()])
  }
}

async function handleSaveToken() {
  await runtime.saveToken()
}
async function handleClearToken() {
  await runtime.clearToken()
}

onMounted(async () => {
  locale.value = getCurrentLocale()
  if (isTauri()) {
    await Promise.all([settings.loadSettings(), runtime.loadStatus()])
  }
})
</script>

<template>
  <div class="space-y-4">
    <div>
      <h1 id="page-title-settings" class="text-lg font-semibold tracking-tight">{{ t('settings.title') }}</h1>
      <p class="text-sm text-muted">{{ t('settings.subtitle') }}</p>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.stockService.title') }}</h2>
        </template>
        <div class="space-y-3">
          <UFormField :label="t('connection.stockService.mainUrl')">
            <UInput v-model="settings.stockUrl.value" type="url" :placeholder="t('connection.stockService.placeholder')" />
          </UFormField>
          <div class="flex flex-wrap gap-2">
            <UButton color="primary" :disabled="!canMutate" icon="i-lucide-save" @click="handleSave">{{ t('connection.stockService.saveSettings') }}</UButton>
            <UButton color="neutral" variant="outline" :disabled="!canMutate" icon="i-lucide-plug-zap" @click="settings.testUrl()">{{ t('connection.stockService.testUrl') }}</UButton>
            <span class="text-sm self-center" :style="{ color: settings.testOk.value === true ? 'var(--ok)' : settings.testOk.value === false ? 'var(--danger)' : '' }">{{ settings.testResult.value }}</span>
          </div>
          <p class="text-xs text-muted">{{ t('connection.stockService.hint') }}</p>
          <p v-if="!canMutate" class="text-xs text-warning">{{ t('banner.fallback') }}</p>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.operatorNetwork.title') }}</h2>
        </template>
        <div class="space-y-3">
          <UFormField :label="t('connection.operatorNetwork.networkMode')">
            <USelect v-model="settings.networkMode.value" :items="networkItems" value-key="value" class="w-full" />
          </UFormField>
          <UFormField :label="t('connection.operatorNetwork.bindAddr')">
            <UInput v-model="settings.bindAddr.value" placeholder="127.0.0.1:5190" />
          </UFormField>
          <UFormField :label="t('connection.operatorNetwork.mcpPath')">
            <UInput v-model="settings.mcpPath.value" placeholder="/mcp" />
          </UFormField>
          <UAlert v-if="settings.showPrivateWarning.value" color="warning" variant="subtle" :title="t('connection.operatorNetwork.warning')" icon="i-lucide-triangle-alert" />
          <UCheckbox v-if="settings.showPrivateWarning.value" v-model="settings.privateAck.value" :label="t('connection.operatorNetwork.ack')" />
          <p class="text-xs text-muted">{{ t('connection.operatorNetwork.hint') }}</p>
          <UButton color="primary" :disabled="!canMutate" icon="i-lucide-save" @click="handleSave">{{ t('common.actions.save') }}</UButton>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.brokerTarget.title') }}</h2>
        </template>
        <div class="space-y-3">
          <UFormField :label="t('connection.brokerTarget.bundleId')">
            <UInput v-model="settings.bundleId.value" placeholder="com.citics.mac.tdx" />
          </UFormField>
          <UFormField :label="t('connection.brokerTarget.processName')">
            <UInput v-model="settings.processName.value" placeholder="中信证券网上交易" />
          </UFormField>
          <div class="grid grid-cols-2 gap-3">
            <UFormField :label="t('connection.brokerTarget.maxDepth')">
              <UInput v-model.number="settings.maxDepth.value" type="number" :min="1" :max="12" />
            </UFormField>
            <UFormField :label="t('connection.brokerTarget.maxNodes')">
              <UInput v-model.number="settings.maxNodes.value" type="number" :min="1" :max="2000" />
            </UFormField>
          </div>
          <UButton color="primary" :disabled="!canMutate" icon="i-lucide-save" @click="handleSave">{{ t('common.actions.save') }}</UButton>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.authentication.title') }}</h2>
        </template>
        <div class="space-y-3">
          <div class="flex flex-wrap items-center gap-2 text-sm">
            <span class="font-medium">{{ t('connection.authentication.tokenLabel') }}</span>
            <UBadge :color="runtime.tokenConfigured.value ? 'success' : 'error'" variant="subtle">{{ runtime.tokenConfigured.value ? t('connection.authentication.configured') : t('connection.authentication.notConfigured') }}</UBadge>
            <span class="text-xs text-muted">({{ runtime.tokenSource.value || t('common.empty') }})</span>
            <span class="inline-flex items-center gap-1 text-xs">
              <span class="h-2 w-2 rounded-full" :class="runtime.serverRunning.value ? 'bg-success' : 'bg-error'"></span>
              {{ runtime.serverRunning.value ? t('connection.authentication.serverRunning') : (runtime.serverError.value ? t('connection.authentication.server', { status: runtime.serverError.value }) : t('connection.authentication.serverNotRunning')) }}
            </span>
          </div>
          <UFormField :label="t('connection.authentication.newToken')">
            <UInput v-model="runtime.newToken.value" type="password" :placeholder="t('connection.authentication.placeholderToken')" />
          </UFormField>
          <div class="flex flex-wrap gap-2">
            <UButton color="primary" :disabled="!canMutate" icon="i-lucide-key-round" @click="handleSaveToken">{{ t('connection.authentication.saveToken') }}</UButton>
            <UButton color="error" variant="outline" :disabled="!canMutate" icon="i-lucide-trash-2" @click="handleClearToken">{{ t('connection.authentication.clearToken') }}</UButton>
          </div>
          <p class="text-xs text-muted">{{ t('connection.authentication.hint') }}</p>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('settings.display.title') }}</h2>
        </template>
        <div class="space-y-4">
          <UFormField :label="t('topbar.language')">
            <USelect :model-value="locale" :items="localeItems" value-key="value" class="w-full" @update:model-value="onLocaleChange($event as AppLocale)" />
          </UFormField>
          <UFormField :label="t('topbar.theme')">
            <USelect :model-value="currentTheme" :items="themeItems" value-key="value" class="w-full" @update:model-value="onThemeChange($event as ThemeMode)" />
          </UFormField>
          <p class="text-xs text-muted">{{ t('settings.display.hint') }}</p>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.statusCard.title') }}</h2>
        </template>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.server') }}</div>
            <div class="font-medium" :class="runtime.serverRunning.value ? 'text-success' : 'text-error'">{{ runtime.serverRunning.value ? t('connection.statusCard.running') : (runtime.serverError.value || t('connection.statusCard.stopped')) }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.bind') }}</div>
            <div class="font-medium truncate">{{ runtime.serverBind.value || settings.bindAddr.value || t('common.empty') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.mcp') }}</div>
            <div class="font-medium truncate">{{ settings.mcpPath.value || t('common.empty') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.accessibility') }}</div>
            <div class="font-medium" :class="runtime.accessibility.value.process_trusted ? 'text-success' : 'text-warning'">{{ runtime.accessibility.value.process_trusted ? t('connection.statusCard.trusted') : t('connection.statusCard.notTrusted') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.target') }}</div>
            <div class="font-medium" :class="runtime.accessibility.value.target_found ? 'text-success' : 'text-warning'">{{ runtime.accessibility.value.target_found ? (runtime.accessibility.value.target_pid ? t('connection.statusCard.foundPid', { pid: String(runtime.accessibility.value.target_pid) }) : t('connection.statusCard.found')) : t('connection.statusCard.notFound') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.restart') }}</div>
            <div class="font-medium" :class="runtime.restartRequired.value ? 'text-warning' : 'text-success'">{{ runtime.restartRequired.value ? t('connection.statusCard.restartRequired') : t('connection.statusCard.restartNotRequired') }}</div>
          </div>
          <div class="col-span-2">
            <div class="text-xs text-muted">{{ t('connection.statusCard.db') }}</div>
            <div class="font-mono text-xs truncate" :title="runtime.dbPath.value">{{ runtime.dbPath.value }}</div>
          </div>
        </div>
        <div v-if="runtime.restartReasons.value.length" class="mt-2 text-xs text-muted">{{ runtime.restartReasons.value.join('; ') }}</div>
        <div v-if="runtime.accessibility.value.notes.length" class="text-xs text-muted">{{ runtime.accessibility.value.notes.join('; ') }}</div>
        <div class="mt-3">
          <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-refresh-cw" @click="runtime.loadStatus()">{{ t('connection.statusCard.refresh') }}</UButton>
        </div>
      </UCard>
    </div>
  </div>
</template>
