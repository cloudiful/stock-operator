import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { isTauri, tauriInvoke } from '@/utils/invoke'
import type { RuntimeStatus } from '@/types/tauri'
import { i18n } from '@/i18n'

// Shared singleton state so Connection overview and Settings stay in sync
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
const instanceIdShort = ref('—')
const newToken = ref('')

let singleInstanceRegistered = false
let singleInstanceUnlisten: (() => void) | null = null

async function fetchAndApplyStatus(tFallback?: (key: string, params?: Record<string, string>) => string) {
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
    return true
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    const t = tFallback ?? ((key: string, p?: Record<string, string>) => {
      try {
        return (i18n.global.t as unknown as (k: string, p?: unknown) => string)(key, p)
      } catch {
        return msg
      }
    })
    // error will be notified by caller if it has onMessage; otherwise silently ignore
    // return false and let caller handle notification
    throw { message: msg, t }
  }
}

export function ensureSingleInstanceListener() {
  if (singleInstanceRegistered) return
  if (!isTauri()) return
  singleInstanceRegistered = true
  try {
    const tauriEvent = (window as unknown as { __TAURI__?: { event?: { listen: (event: string, cb: () => void) => Promise<unknown> } } }).__TAURI__?.event
    if (!tauriEvent?.listen) return
    const maybePromise = tauriEvent.listen('single-instance', () => {
      void fetchAndApplyStatus().catch(() => {})
    }) as unknown
    // Tauri v2 listen returns Promise<UnlistenFn>
    if (maybePromise && typeof (maybePromise as Promise<unknown>).then === 'function') {
      ;(maybePromise as Promise<unknown>).then((unlisten) => {
        if (typeof unlisten === 'function') singleInstanceUnlisten = unlisten as () => void
      }).catch(() => {})
    } else if (typeof maybePromise === 'function') {
      singleInstanceUnlisten = maybePromise as () => void
    }
  } catch {}
}

export function useRuntime(onMessage?: (text: string, kind: 'ok' | 'err' | 'info') => void) {
  const { t } = useI18n()

  const canMutate = computed(() => isTauri())

  async function loadStatus() {
    try {
      await fetchAndApplyStatus(t as unknown as (k: string, p?: Record<string, string>) => string)
    } catch (e: unknown) {
      const err = e as { message: string; t: (k: string, p?: unknown) => string }
      const msg = err?.message ?? (e instanceof Error ? e.message : String(e))
      const translate = err?.t ?? t
      try {
        onMessage?.(translate('connection.statusCard.loadFailed', { msg } as unknown as Record<string, unknown>), 'err')
      } catch {
        onMessage?.(msg, 'err')
      }
    }
  }

  async function saveToken() {
    if (!newToken.value.trim()) {
      onMessage?.(t('connection.authentication.emptyToken'), 'err')
      return { ok: false }
    }
    try {
      await tauriInvoke('save_token', { token: newToken.value })
      newToken.value = ''
      onMessage?.(t('connection.authentication.saved'), 'ok')
      await loadStatus()
      return { ok: true }
    } catch (e: unknown) {
      const msg = typeof e === 'string' ? e : (e as Error)?.message ?? String(e)
      onMessage?.(t('connection.authentication.saveFailed', { msg }), 'err')
      return { ok: false }
    }
  }

  async function clearToken() {
    try {
      await tauriInvoke('clear_token')
      onMessage?.(t('connection.authentication.cleared'), 'ok')
      await loadStatus()
      return { ok: true }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e)
      onMessage?.(t('connection.authentication.clearFailed', { msg }), 'err')
      return { ok: false }
    }
  }

  return {
    tokenConfigured,
    tokenSource,
    serverRunning,
    serverError,
    serverBind,
    accessibility,
    restartRequired,
    restartReasons,
    dbPath,
    instanceIdShort,
    newToken,
    canMutate,
    loadStatus,
    saveToken,
    clearToken,
  }
}

// For testing: allow resetting singleton in tests if needed
export function __resetRuntimeStateForTest() {
  tokenConfigured.value = false
  tokenSource.value = ''
  serverRunning.value = false
  serverError.value = null
  serverBind.value = null
  accessibility.value = { process_trusted: false, target_found: false, target_pid: null, notes: [] }
  restartRequired.value = false
  restartReasons.value = []
  dbPath.value = '—'
  instanceIdShort.value = '—'
  newToken.value = ''
  singleInstanceRegistered = false
  if (singleInstanceUnlisten) {
    try { singleInstanceUnlisten() } catch {}
    singleInstanceUnlisten = null
  }
}
