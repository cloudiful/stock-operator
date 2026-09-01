import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { isTauri, tauriInvoke } from '@/utils/invoke'
import type { PublicSettings, SaveSettingsRequest, TestConnectionResult } from '@/types/tauri'

// Shared singleton state so ConnectionPage and SettingsPage reflect the same data without manual refresh
const stockUrl = ref('')
const bindAddr = ref('')
const mcpPath = ref('')
const bundleId = ref('')
const processName = ref('')
const maxDepth = ref(6)
const maxNodes = ref(300)
const networkMode = ref('loopback')
const privateAck = ref(false)
const instanceIdShort = ref('—')

const testResult = ref('')
const testOk = ref<boolean | null>(null)

export function useSettings(onMessage?: (text: string, kind: 'ok' | 'err' | 'info') => void) {
  const { t } = useI18n()

  const showPrivateWarning = computed(() => networkMode.value === 'private-overlay')
  const canMutate = computed(() => isTauri())

  async function loadSettings() {
    try {
      const s = await tauriInvoke<PublicSettings>('get_settings')
      stockUrl.value = s.stock_service_url ?? ''
      bindAddr.value = s.bind_addr ?? ''
      mcpPath.value = s.mcp_path ?? ''
      bundleId.value = s.target_bundle_id ?? ''
      processName.value = s.target_process_name ?? ''
      maxDepth.value = s.max_depth
      maxNodes.value = s.max_nodes
      networkMode.value = s.network_mode ?? 'loopback'
      privateAck.value = false
      instanceIdShort.value = (s.instance_id || '').slice(0, 8) || '—'
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : typeof e === 'string' ? e : JSON.stringify(e)
      onMessage?.(t('message.loadSettingsFailed', { msg }), 'err')
    }
  }

  async function saveSettings() {
    const req: SaveSettingsRequest = {
      stock_service_url: stockUrl.value.trim() ? stockUrl.value.trim() : null,
      bind_addr: bindAddr.value.trim(),
      mcp_path: mcpPath.value.trim(),
      target_bundle_id: bundleId.value.trim(),
      target_process_name: processName.value.trim(),
      max_depth: Number(maxDepth.value),
      max_nodes: Number(maxNodes.value),
      network_mode: networkMode.value,
      private_overlay_ack: privateAck.value,
    }
    try {
      const resp = await tauriInvoke<{ message: string; restart_required: boolean }>('save_settings', {
        request: req as unknown as Record<string, unknown>,
      })
      const message = (resp as unknown as { message?: string }).message ?? t('connection.statusCard.saved')
      const needsRestart = (resp as unknown as { restart_required?: boolean }).restart_required ?? false
      onMessage?.(
        message || (needsRestart ? t('connection.statusCard.savedRestart') : t('connection.statusCard.saved')),
        needsRestart ? 'info' : 'ok',
      )
      return { ok: true, needsRestart }
    } catch (e: unknown) {
      const msg = typeof e === 'string' ? e : (e as Error)?.message ?? JSON.stringify(e)
      onMessage?.(t('connection.statusCard.saveFailed', { msg }), 'err')
      return { ok: false }
    }
  }

  async function testUrl() {
    const url = stockUrl.value.trim()
    if (!url) {
      onMessage?.(t('connection.stockService.enterUrl'), 'err')
      testResult.value = t('connection.stockService.enterUrl')
      testOk.value = false
      return
    }
    testResult.value = t('connection.stockService.testing')
    testOk.value = null
    try {
      const r = await tauriInvoke<TestConnectionResult>('test_stock_service_url', { url })
      const base = r.ok
        ? t('connection.stockService.reachable', { status: String(r.status ?? '') })
        : r.status
          ? t('connection.stockService.returned', { status: String(r.status) })
          : r.message
      const latency = r.latency_ms ? ` (${r.latency_ms} ms)` : ''
      const msg = r.ok || r.status ? base + latency : t('connection.stockService.failed', { msg: r.message }) + latency
      testResult.value = msg
      testOk.value = r.ok
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      testResult.value = t('connection.stockService.failed', { msg })
      testOk.value = false
    }
  }

  return {
    stockUrl,
    bindAddr,
    mcpPath,
    bundleId,
    processName,
    maxDepth,
    maxNodes,
    networkMode,
    privateAck,
    instanceIdShort,
    testResult,
    testOk,
    showPrivateWarning,
    canMutate,
    loadSettings,
    saveSettings,
    testUrl,
  }
}

export function __resetSettingsStateForTest() {
  stockUrl.value = ''
  bindAddr.value = ''
  mcpPath.value = ''
  bundleId.value = ''
  processName.value = ''
  maxDepth.value = 6
  maxNodes.value = 300
  networkMode.value = 'loopback'
  privateAck.value = false
  instanceIdShort.value = '—'
  testResult.value = ''
  testOk.value = null
}
