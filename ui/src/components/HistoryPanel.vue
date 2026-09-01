<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { tauriInvoke, isTauri } from '@/utils/invoke'
import { fmtTime, shortFingerprint, payloadSummary, payloadTitle } from '@/utils/format'
import type { OperationHistoryResponse, AuditHistoryResponse } from '@/types/tauri'

const { t } = useI18n()
const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

const fKind = ref('ANY')
const fState = ref('ANY')
const fLimit = ref(20)
const fAuditLimit = ref(20)
const fOpId = ref('')

const ops = ref<OperationHistoryResponse['operations']>([])
const opsTotal = ref(0)
const opsOffset = ref(0)
const audit = ref<AuditHistoryResponse['events']>([])
const auditTotal = ref(0)
const auditOffset = ref(0)

const opsMeta = ref(t('common.loading'))
const auditMeta = ref(t('common.loading'))

const kindItems = computed(() => [
  { label: t('history.operations.kindAny'), value: 'ANY' },
  { label: 'submit_order', value: 'submit_order' },
  { label: 'cancel_order', value: 'cancel_order' },
])
const stateItems = computed(() => [
  { label: t('history.operations.stateAny'), value: 'ANY' },
  { label: 'confirmation_opened', value: 'confirmation_opened' },
  { label: 'confirming', value: 'confirming' },
  { label: 'confirmed', value: 'confirmed' },
  { label: 'unknown', value: 'unknown' },
  { label: 'expired', value: 'expired' },
  { label: 'aborted', value: 'aborted' },
])
const limitItems = [20, 50, 100].map((n) => ({ label: String(n), value: n }))

async function loadOps() {
  const limit = fLimit.value
  const offset = opsOffset.value
  const kind = fKind.value && fKind.value !== 'ANY' ? fKind.value : null
  const stateFilter = fState.value && fState.value !== 'ANY' ? fState.value : null
  opsMeta.value = t('common.loading')
  try {
    const resp = await tauriInvoke<OperationHistoryResponse>('list_operations', {
      limit,
      offset,
      kind,
      stateFilter,
    } as unknown as Record<string, unknown>)
    opsTotal.value = resp.total
    ops.value = resp.operations ?? []
    opsMeta.value = t('history.operations.total', { total: String(resp.total) })
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    opsMeta.value = t('history.confirm.loadFailed', { msg: msg.slice(0, 200) })
    ops.value = []
  }
}

async function loadAudit() {
  const limit = fAuditLimit.value
  const offset = auditOffset.value
  const operationId = fOpId.value.trim() || null
  auditMeta.value = t('common.loading')
  try {
    const resp = await tauriInvoke<AuditHistoryResponse>('list_audit_events', {
      limit,
      offset,
      operationId,
    } as unknown as Record<string, unknown>)
    auditTotal.value = resp.total
    audit.value = resp.events ?? []
    auditMeta.value = t('history.audit.total', { total: String(resp.total) })
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    auditMeta.value = t('history.confirm.loadFailed', { msg: msg.slice(0, 200) })
    audit.value = []
  }
}

async function resolveStale(opId: string, currentState: string) {
  if (!isTauri()) {
    emit('message', t('banner.fallback'), 'info')
    return
  }
  const idShort = opId.slice(0, 8)
  const confirmed = window.confirm(
    `${t('history.confirm.resolveTitle', { id: idShort, state: currentState })}\n\n${t('history.confirm.resolveBody')}`,
  )
  if (!confirmed) return
  try {
    const res = await tauriInvoke<{ message?: string }>('resolve_stale_operation', {
      operation_id: opId,
      operationId: opId,
    } as unknown as Record<string, unknown>)
    const msg = (res as { message?: string }).message ?? t('history.confirm.resolved', { id: idShort })
    emit('message', msg, 'ok')
    await loadOps()
    await loadAudit()
  } catch (e: unknown) {
    const raw = typeof e === 'string' ? e : (e as Error)?.message ?? JSON.stringify(e)
    emit('message', t('history.confirm.failed', { msg: raw.slice(0, 300) }), 'err')
  }
}

function opsPageText(): string {
  const limit = fLimit.value || 20
  return t('history.operations.page', { page: String(Math.floor(opsOffset.value / limit) + 1), rows: String(ops.value.length) })
}
function auditPageText(): string {
  const limit = fAuditLimit.value || 20
  return t('history.audit.page', { page: String(Math.floor(auditOffset.value / limit) + 1), rows: String(audit.value.length) })
}

function opsPrev() {
  opsOffset.value = Math.max(0, opsOffset.value - (fLimit.value || 20))
  loadOps()
}
function opsNext() {
  const limit = fLimit.value || 20
  if (opsOffset.value + limit < opsTotal.value) {
    opsOffset.value += limit
    loadOps()
  }
}
function auditPrev() {
  auditOffset.value = Math.max(0, auditOffset.value - (fAuditLimit.value || 20))
  loadAudit()
}
function auditNext() {
  const limit = fAuditLimit.value || 20
  if (auditOffset.value + limit < auditTotal.value) {
    auditOffset.value += limit
    loadAudit()
  }
}

onMounted(() => {
  loadOps()
  loadAudit()
})

defineExpose({ loadOps, loadAudit })
</script>

<template>
  <div class="space-y-4">
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('history.operations.title') }}</h2>
          <span class="text-xs text-muted">{{ opsMeta }}</span>
        </div>
      </template>
      <div class="flex flex-wrap gap-3 items-end mb-3">
        <UFormField :label="t('history.operations.kind')" class="min-w-[140px]">
          <USelect v-model="fKind" :items="kindItems" value-key="value" class="w-full" @update:model-value="opsOffset = 0; loadOps()" />
        </UFormField>
        <UFormField :label="t('history.operations.state')" class="min-w-[160px]">
          <USelect v-model="fState" :items="stateItems" value-key="value" class="w-full" @update:model-value="opsOffset = 0; loadOps()" />
        </UFormField>
        <UFormField :label="t('history.operations.limit')" class="min-w-[100px]">
          <USelect v-model="fLimit" :items="limitItems" value-key="value" class="w-full" @update:model-value="opsOffset = 0; loadOps()" />
        </UFormField>
        <UButton color="primary" icon="i-lucide-refresh-cw" @click="opsOffset = 0; loadOps()">{{ t('history.operations.refresh') }}</UButton>
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{{ t('history.operations.time') }}</th>
              <th>{{ t('history.operations.kind') }}</th>
              <th>{{ t('history.operations.state') }}</th>
              <th>{{ t('history.operations.fingerprint') }}</th>
              <th>{{ t('history.operations.payload') }}</th>
              <th>{{ t('history.operations.actor') }}</th>
              <th>{{ t('history.operations.action') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="ops.length === 0"><td colspan="7" class="empty">{{ t('history.operations.empty') }}</td></tr>
            <tr v-for="o in ops" :key="(o.operation_id || o.id || '') + o.created_at">
              <td>{{ fmtTime(o.created_at) }}</td>
              <td>{{ o.kind || t('common.empty') }}</td>
              <td><UBadge :color="o.state === 'unknown' || o.state === 'expired' ? 'warning' : o.state === 'confirmed' ? 'success' : 'neutral'" variant="subtle" size="sm">{{ o.state || t('common.empty') }}</UBadge></td>
              <td :title="o.fingerprint || ''">{{ shortFingerprint(o.fingerprint) }}</td>
              <td :title="payloadTitle(o.payload_summary)">{{ payloadSummary(o.payload_summary) }}</td>
              <td>{{ o.actor_source || t('common.empty') }}</td>
              <td>
                <UButton
                  v-if="o.state === 'unknown' || o.state === 'expired'"
                  color="error"
                  variant="soft"
                  size="xs"
                  icon="i-lucide-circle-alert"
                  :title="t('history.operations.resolveTitle')"
                  :disabled="!isTauri()"
                  @click="resolveStale((o.operation_id || o.id) as string, o.state)"
                >
                  {{ t('history.operations.resolve') }}
                </UButton>
                <span v-else class="text-xs text-muted">{{ t('history.operations.noAction') }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="text-xs text-muted mt-2">{{ t('history.operations.staleHint') }}</p>
      <div class="flex items-center justify-end gap-2 mt-3">
        <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-chevron-left" @click="opsPrev">{{ t('history.pager.prev') }}</UButton>
        <span class="text-xs text-muted">{{ opsPageText() }}</span>
        <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-chevron-right" trailing @click="opsNext">{{ t('history.pager.next') }}</UButton>
      </div>
    </UCard>

    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <h2 class="text-xs font-semibold uppercase tracking-wide text-muted">{{ t('history.audit.title') }}</h2>
          <span class="text-xs text-muted">{{ auditMeta }}</span>
        </div>
      </template>
      <div class="flex flex-wrap gap-3 items-end mb-3">
        <UFormField :label="t('history.audit.operationId')" class="min-w-[200px] flex-1">
          <UInput v-model="fOpId" :placeholder="t('history.audit.placeholder')" @change="auditOffset = 0" />
        </UFormField>
        <UFormField :label="t('history.audit.limit')" class="min-w-[100px]">
          <USelect v-model="fAuditLimit" :items="limitItems" value-key="value" class="w-full" @update:model-value="auditOffset = 0; loadAudit()" />
        </UFormField>
        <UButton color="primary" icon="i-lucide-refresh-cw" @click="auditOffset = 0; loadAudit()">{{ t('history.audit.refresh') }}</UButton>
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{{ t('history.audit.time') }}</th>
              <th>{{ t('history.audit.op') }}</th>
              <th>{{ t('history.audit.type') }}</th>
              <th>{{ t('history.audit.transition') }}</th>
              <th>{{ t('history.audit.actor') }}</th>
              <th>{{ t('history.audit.detail') }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="audit.length === 0"><td colspan="6" class="empty">{{ t('history.audit.empty') }}</td></tr>
            <tr v-for="ev in audit" :key="(ev.operation_id || '') + ev.created_at + ev.event_type">
              <td>{{ fmtTime(ev.created_at) }}</td>
              <td :title="ev.operation_id || ''">{{ ev.operation_id ? ev.operation_id.slice(0, 8) + '…' : t('common.empty') }}</td>
              <td>{{ ev.event_type || t('common.empty') }}</td>
              <td>{{ (ev.from_state || t('common.empty')) + ' → ' + (ev.to_state || t('common.empty')) }}</td>
              <td>{{ ev.actor_source || t('common.empty') }}</td>
              <td>{{ payloadSummary(ev.detail) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <div class="flex items-center justify-end gap-2 mt-3">
        <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-chevron-left" @click="auditPrev">{{ t('history.pager.prev') }}</UButton>
        <span class="text-xs text-muted">{{ auditPageText() }}</span>
        <UButton color="neutral" variant="outline" size="sm" icon="i-lucide-chevron-right" trailing @click="auditNext">{{ t('history.pager.next') }}</UButton>
      </div>
    </UCard>
  </div>
</template>

<style scoped>
.table-wrap { overflow:auto; border:1px solid var(--border); border-radius:8px; }
table { width:100%; border-collapse: collapse; font-size:13px; }
th { text-align:left; font-weight:700; color: var(--muted); background: var(--table-head-bg); padding:8px 10px; border-bottom:1px solid var(--border); white-space: nowrap; }
td { padding:8px 10px; border-bottom:1px solid var(--table-row-border); vertical-align: top; }
tr:last-child td { border-bottom:none; }
.empty { text-align:center; color: var(--muted); padding:18px !important; }
</style>
