<script setup lang="ts">
import { onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSettings } from '@/composables/useSettings'
import { useRuntime } from '@/composables/useRuntime'
import { isTauri } from '@/utils/invoke'

const { t } = useI18n()
const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

function onMessage(text: string, kind: 'ok' | 'err' | 'info') {
  emit('message', text, kind)
}

const settings = useSettings(onMessage)
const runtime = useRuntime(onMessage)

const canMutate = isTauri()

async function refreshAll() {
  await Promise.all([settings.loadSettings(), runtime.loadStatus()])
}

onMounted(async () => {
  if (isTauri()) await refreshAll()
})
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between gap-3">
      <h1 id="page-title-connection" class="text-lg font-semibold tracking-tight">{{ t('connection.overview.title') }}</h1>
      <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-refresh-cw" @click="refreshAll">
        {{ t('connection.statusCard.refresh') }}
      </UButton>
    </div>
    <p class="text-sm text-muted">{{ t('connection.overview.subtitle') }}</p>

    <div class="grid gap-4 md:grid-cols-2">
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.stockService.title') }}</h2>
            <UBadge :color="settings.testOk.value === true ? 'success' : settings.testOk.value === false ? 'error' : 'neutral'" variant="subtle" size="sm">
              {{ settings.testOk.value === true ? t('connection.overview.testOk') : settings.testOk.value === false ? t('connection.overview.testFail') : t('connection.overview.testIdle') }}
            </UBadge>
          </div>
        </template>
        <div class="space-y-3">
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium">{{ t('connection.stockService.mainUrl') }}</span>
            <div class="flex items-center gap-2 rounded-lg border border-default bg-elevated/50 px-3 py-2">
              <span class="flex-1 truncate text-sm" :title="settings.stockUrl.value || t('common.empty')">{{ settings.stockUrl.value || t('common.empty') }}</span>
              <UBadge v-if="settings.stockUrl.value" variant="subtle" color="neutral" size="sm">URL</UBadge>
            </div>
            <span class="text-xs text-muted">{{ t('connection.stockService.hint') }}</span>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <UButton color="primary" :disabled="!canMutate" icon="i-lucide-plug-zap" @click="settings.testUrl()">{{ t('connection.stockService.testUrl') }}</UButton>
            <span class="text-sm" :class="settings.testOk.value === true ? 'text-success' : settings.testOk.value === false ? 'text-error' : 'text-muted'">{{ settings.testResult.value }}</span>
          </div>
          <p v-if="!canMutate" class="text-xs text-muted">{{ t('banner.fallback') }}</p>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.authentication.title') }}</h2>
        </template>
        <div class="space-y-3 text-sm">
          <div class="flex flex-wrap items-center gap-2">
            <span class="font-medium">{{ t('connection.authentication.tokenLabel') }}</span>
            <UBadge :color="runtime.tokenConfigured.value ? 'success' : 'error'" variant="subtle">{{ runtime.tokenConfigured.value ? t('connection.authentication.configured') : t('connection.authentication.notConfigured') }}</UBadge>
            <span class="text-xs text-muted">({{ runtime.tokenSource.value || t('common.empty') }})</span>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <span class="font-medium">{{ t('connection.statusCard.server') }}</span>
            <span class="inline-flex items-center gap-1.5">
              <span class="h-2.5 w-2.5 rounded-full" :class="runtime.serverRunning.value ? 'bg-success' : 'bg-error'"></span>
              <span :class="runtime.serverRunning.value ? 'text-success' : 'text-error'">{{ runtime.serverRunning.value ? t('connection.statusCard.running') : (runtime.serverError.value || t('connection.statusCard.stopped')) }}</span>
            </span>
            <UBadge v-if="runtime.serverBind.value" color="neutral" variant="outline" size="sm">{{ runtime.serverBind.value }}</UBadge>
          </div>
          <p class="text-xs text-muted">{{ t('connection.authentication.hint') }}</p>
        </div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.statusCard.title') }}</h2>
        </template>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.bind') }}</div>
            <div class="font-medium truncate" :title="runtime.serverBind.value || settings.bindAddr.value || t('common.empty')">{{ runtime.serverBind.value || settings.bindAddr.value || t('common.empty') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.mcp') }}</div>
            <div class="font-medium truncate">{{ settings.mcpPath.value || t('common.empty') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.accessibility') }}</div>
            <div class="font-medium flex items-center gap-1.5" :class="runtime.accessibility.value.process_trusted ? 'text-success' : 'text-warning'">
              <span class="h-2 w-2 rounded-full" :class="runtime.accessibility.value.process_trusted ? 'bg-success' : 'bg-warning'"></span>
              {{ runtime.accessibility.value.process_trusted ? t('connection.statusCard.trusted') : t('connection.statusCard.notTrusted') }}
            </div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.target') }}</div>
            <div class="font-medium" :class="runtime.accessibility.value.target_found ? 'text-success' : 'text-warning'">
              {{ runtime.accessibility.value.target_found ? (runtime.accessibility.value.target_pid ? t('connection.statusCard.foundPid', { pid: String(runtime.accessibility.value.target_pid) }) : t('connection.statusCard.found')) : t('connection.statusCard.notFound') }}
            </div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.restart') }}</div>
            <div class="font-medium" :class="runtime.restartRequired.value ? 'text-warning' : 'text-success'">{{ runtime.restartRequired.value ? t('connection.statusCard.restartRequired') : t('connection.statusCard.restartNotRequired') }}</div>
          </div>
          <div>
            <div class="text-xs text-muted">{{ t('connection.statusCard.db') }}</div>
            <div class="font-mono text-xs truncate" :title="runtime.dbPath.value">{{ runtime.dbPath.value }}</div>
          </div>
        </div>
        <div v-if="runtime.restartReasons.value.length" class="mt-3 text-xs text-muted">{{ runtime.restartReasons.value.join('; ') }}</div>
        <div v-if="runtime.accessibility.value.notes.length" class="mt-1 text-xs text-muted">{{ runtime.accessibility.value.notes.join('; ') }}</div>
      </UCard>

      <UCard>
        <template #header>
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('connection.overview.runtimeTitle') }}</h2>
        </template>
        <div class="space-y-2 text-sm">
          <div class="flex items-center justify-between">
            <span class="text-muted">{{ t('topbar.instance') }}</span>
            <UBadge color="neutral" variant="outline">{{ runtime.instanceIdShort.value || settings.instanceIdShort.value }}</UBadge>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-muted">{{ t('connection.overview.networkModeLabel') }}</span>
            <UBadge :color="settings.networkMode.value === 'private-overlay' ? 'warning' : 'neutral'" variant="subtle">{{ settings.networkMode.value }}</UBadge>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-muted">{{ t('connection.brokerTarget.title') }}</span>
            <span class="font-medium truncate max-w-[160px]" :title="settings.bundleId.value || settings.processName.value || t('common.empty')">{{ settings.bundleId.value || settings.processName.value || t('common.empty') }}</span>
          </div>
          <p class="text-xs text-muted pt-2">{{ t('connection.overview.hint') }}</p>
        </div>
      </UCard>
    </div>
  </div>
</template>
