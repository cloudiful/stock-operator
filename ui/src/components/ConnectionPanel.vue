<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { tauriInvoke, isTauri } from '@/utils/invoke'
import type {
  PublicSettings,
  RuntimeStatus,
  SaveSettingsRequest,
  TestConnectionResult,
} from '@/types/tauri'

const { t } = useI18n()

const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

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

const tokenConfigured = ref(false)
const tokenSource = ref('')
const serverRunning = ref(false)
const serverError = ref<string | null>(null)
const serverBind = ref<string | null>(null)
const accessibility = ref({
  process_trusted: false,
  target_found: false,
  target_pid: null as number | null,
  notes: [] as string[],
})
const restartRequired = ref(false)
const restartReasons = ref<string[]>([])
const dbPath = ref('—')

const testResult = ref('')
const testOk = ref<boolean | null>(null)

const showPrivateWarning = computed(() => networkMode.value === 'private-overlay')
const canMutate = computed(() => isTauri())

function updateNetworkUI() {
  // handled via computed; keep for parity
}

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
    emit('message', t('message.loadSettingsFailed', { msg }), 'err')
  }
}

async function loadStatus() {
  try {
    const st = await tauriInvoke<RuntimeStatus>('get_runtime_status')
    tokenConfigured.value = st.token_configured
    tokenSource.value = st.token_source
    serverRunning.value = st.server_running
    serverError.value = st.server_error
    serverBind.value = st.server_bind_addr
    accessibility.value = {
      process_trusted: st.accessibility.process_trusted,
      target_found: st.accessibility.target_found,
      target_pid: st.accessibility.target_pid,
      notes: st.accessibility.notes ?? [],
    }
    restartRequired.value = st.restart_required
    restartReasons.value = st.restart_reasons ?? []
    dbPath.value = st.db_path ?? '—'
    instanceIdShort.value = (st.instance_id || '').slice(0, 8) || instanceIdShort.value
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    emit('message', t('connection.statusCard.loadFailed', { msg }), 'err')
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
    // Compatibility: Rust returns SaveSettingsResponse with message and restart_required flag
    const message = (resp as unknown as { message?: string }).message ?? t('connection.statusCard.saved')
    const needsRestart = (resp as unknown as { restart_required?: boolean }).restart_required ?? false
    emit('message', message || (needsRestart ? t('connection.statusCard.savedRestart') : t('connection.statusCard.saved')), needsRestart ? 'info' : 'ok')
    await loadStatus()
    await loadSettings()
  } catch (e: unknown) {
    const msg = typeof e === 'string' ? e : (e as Error)?.message ?? JSON.stringify(e)
    emit('message', t('connection.statusCard.saveFailed', { msg }), 'err')
  }
}

async function testUrl() {
  const url = stockUrl.value.trim()
  if (!url) {
    emit('message', t('connection.stockService.enterUrl'), 'err')
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
    // For failed case where ok false but status null, show message
    const msg = r.ok || r.status ? base + latency : t('connection.stockService.failed', { msg: r.message }) + latency
    testResult.value = msg
    testOk.value = r.ok
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    testResult.value = t('connection.stockService.failed', { msg })
    testOk.value = false
  }
}

const newToken = ref('')

async function saveToken() {
  if (!newToken.value.trim()) {
    emit('message', t('connection.authentication.emptyToken'), 'err')
    return
  }
  try {
    await tauriInvoke('save_token', { token: newToken.value })
    newToken.value = ''
    emit('message', t('connection.authentication.saved'), 'ok')
    await loadStatus()
  } catch (e: unknown) {
    const msg = typeof e === 'string' ? e : (e as Error)?.message ?? String(e)
    emit('message', t('connection.authentication.saveFailed', { msg }), 'err')
  }
}

async function clearToken() {
  try {
    await tauriInvoke('clear_token')
    emit('message', t('connection.authentication.cleared'), 'ok')
    await loadStatus()
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    emit('message', t('connection.authentication.clearFailed', { msg }), 'err')
  }
}

watch(networkMode, () => updateNetworkUI())

onMounted(async () => {
  if (isTauri()) {
    await loadSettings()
    await loadStatus()
    // Listen for single-instance event to refresh
    try {
      const tauriEvent = (window as unknown as { __TAURI__?: { event?: { listen: (event: string, cb: () => void) => Promise<unknown> } } }).__TAURI__?.event
      if (tauriEvent?.listen) {
        await tauriEvent.listen('single-instance', () => {
          loadStatus()
        })
      }
    } catch {}
  }
})

defineExpose({ loadSettings, loadStatus, instanceIdShort })
</script>

<template>
  <div class="grid">
    <section class="card">
      <h2>{{ t('connection.stockService.title') }}</h2>
      <label>{{ t('connection.stockService.mainUrl') }}
        <input v-model="stockUrl" type="url" :placeholder="t('connection.stockService.placeholder')" autocomplete="off" />
      </label>
      <div class="row">
        <button class="primary" :disabled="!canMutate" @click="saveSettings">{{ t('connection.stockService.saveSettings') }}</button>
        <button :disabled="!canMutate" @click="testUrl">{{ t('connection.stockService.testUrl') }}</button>
        <span class="hint" :style="{ color: testOk === true ? 'var(--ok)' : testOk === false ? 'var(--danger)' : '' }">{{ testResult }}</span>
      </div>
      <p class="hint">{{ t('connection.stockService.hint') }}</p>
    </section>

    <section class="card">
      <h2>{{ t('connection.operatorNetwork.title') }}</h2>
      <label>{{ t('connection.operatorNetwork.networkMode') }}
        <select v-model="networkMode">
          <option value="loopback">{{ t('connection.operatorNetwork.loopback') }}</option>
          <option value="private-overlay">{{ t('connection.operatorNetwork.privateOverlay') }}</option>
        </select>
      </label>
      <label>{{ t('connection.operatorNetwork.bindAddr') }}
        <input v-model="bindAddr" type="text" placeholder="127.0.0.1:5190" />
      </label>
      <label>{{ t('connection.operatorNetwork.mcpPath') }}
        <input v-model="mcpPath" type="text" placeholder="/mcp" />
      </label>
      <div v-if="showPrivateWarning" class="warning">{{ t('connection.operatorNetwork.warning') }}</div>
      <label v-if="showPrivateWarning" class="ack"><input type="checkbox" v-model="privateAck" /> {{ t('connection.operatorNetwork.ack') }}</label>
      <p class="hint">{{ t('connection.operatorNetwork.hint') }}</p>
    </section>

    <section class="card">
      <h2>{{ t('connection.brokerTarget.title') }}</h2>
      <label>{{ t('connection.brokerTarget.bundleId') }}
        <input v-model="bundleId" type="text" placeholder="com.citics.mac.tdx" />
      </label>
      <label>{{ t('connection.brokerTarget.processName') }}
        <input v-model="processName" type="text" placeholder="中信证券网上交易" />
      </label>
      <div class="row two">
        <label>{{ t('connection.brokerTarget.maxDepth') }}
          <input v-model.number="maxDepth" type="number" min="1" max="12" />
        </label>
        <label>{{ t('connection.brokerTarget.maxNodes') }}
          <input v-model.number="maxNodes" type="number" min="1" max="2000" />
        </label>
      </div>
    </section>

    <section class="card">
      <h2>{{ t('connection.authentication.title') }}</h2>
      <div class="status-row">
        <span>{{ t('connection.authentication.tokenLabel') }}</span>
        <span class="badge" :style="tokenConfigured ? 'background:#dcfce7;border-color:#bbf7d0' : 'background:#fee2e2;border-color:#fecaca'">{{ tokenConfigured ? t('connection.authentication.configured') : t('connection.authentication.notConfigured') }}</span>
        <span class="hint">({{ tokenSource }})</span>
        <span class="dot" :class="serverRunning ? 'ok' : serverError ? 'err' : 'err'" :title="serverRunning ? t('connection.authentication.serverRunning') : serverError || t('connection.authentication.serverNotRunning')"></span>
        <span class="hint">{{ serverRunning ? t('connection.authentication.serverRunning') : serverError ? t('connection.authentication.server', { status: serverError }) : t('connection.authentication.serverNotRunning') }}</span>
      </div>
      <label>{{ t('connection.authentication.newToken') }}
        <input v-model="newToken" type="password" :placeholder="t('connection.authentication.placeholderToken')" autocomplete="off" />
      </label>
      <div class="row">
        <button class="primary" :disabled="!canMutate" @click="saveToken">{{ t('connection.authentication.saveToken') }}</button>
        <button class="danger" :disabled="!canMutate" @click="clearToken">{{ t('connection.authentication.clearToken') }}</button>
      </div>
      <p class="hint">{{ t('connection.authentication.hint') }}</p>
    </section>

    <section class="card status-card">
      <h2>{{ t('connection.statusCard.title') }}</h2>
      <div class="status-grid">
        <div><span class="k">{{ t('connection.statusCard.server') }}</span><span class="v" :class="serverRunning ? 'ok-text' : 'err-text'">{{ serverRunning ? t('connection.statusCard.running') : (serverError || t('connection.statusCard.stopped')) }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.bind') }}</span><span class="v">{{ serverBind || bindAddr || t('common.empty') }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.mcp') }}</span><span class="v">{{ mcpPath || t('common.empty') }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.accessibility') }}</span><span class="v" :class="accessibility.process_trusted ? 'ok-text' : 'warn-text'">{{ accessibility.process_trusted ? t('connection.statusCard.trusted') : t('connection.statusCard.notTrusted') }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.target') }}</span><span class="v" :class="accessibility.target_found ? 'ok-text' : 'warn-text'">{{ accessibility.target_found ? (accessibility.target_pid ? t('connection.statusCard.foundPid', { pid: String(accessibility.target_pid) }) : t('connection.statusCard.found')) : t('connection.statusCard.notFound') }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.restart') }}</span><span class="v" :class="restartRequired ? 'warn-text' : 'ok-text'">{{ restartRequired ? t('connection.statusCard.restartRequired') : t('connection.statusCard.restartNotRequired') }}</span></div>
        <div><span class="k">{{ t('connection.statusCard.db') }}</span><span class="v small">{{ dbPath }}</span></div>
      </div>
      <div class="hint">{{ restartReasons.join('; ') }}</div>
      <div class="hint">{{ accessibility.notes.join('; ') }}</div>
      <div class="row">
        <button @click="loadStatus">{{ t('connection.statusCard.refresh') }}</button>
      </div>
    </section>
  </div>
</template>
