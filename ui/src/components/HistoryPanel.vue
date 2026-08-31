<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { tauriInvoke } from '@/utils/invoke'
import { fmtTime, shortFingerprint, payloadSummary, payloadTitle } from '@/utils/format'
import type { OperationHistoryResponse, AuditHistoryResponse } from '@/types/tauri'

const { t } = useI18n()
const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

const fKind = ref('')
const fState = ref('')
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

async function loadOps() {
  const limit = fLimit.value
  const offset = opsOffset.value
  const kind = fKind.value || null
  const stateFilter = fState.value || null
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
  <div class="stack">
    <section class="card">
      <h2>{{ t('history.operations.title') }}</h2>
      <div class="filters">
        <label>{{ t('history.operations.kind') }}
          <select v-model="fKind" @change="opsOffset = 0; loadOps()">
            <option value="">{{ t('history.operations.kindAny') }}</option>
            <option value="submit_order">submit_order</option>
            <option value="cancel_order">cancel_order</option>
          </select>
        </label>
        <label>{{ t('history.operations.state') }}
          <select v-model="fState" @change="opsOffset = 0; loadOps()">
            <option value="">{{ t('history.operations.stateAny') }}</option>
            <option value="confirmation_opened">confirmation_opened</option>
            <option value="confirming">confirming</option>
            <option value="confirmed">confirmed</option>
            <option value="unknown">unknown</option>
            <option value="expired">expired</option>
            <option value="aborted">aborted</option>
          </select>
        </label>
        <label>{{ t('history.operations.limit') }}
          <select v-model.number="fLimit" @change="opsOffset = 0; loadOps()">
            <option :value="20">20</option>
            <option :value="50">50</option>
            <option :value="100">100</option>
          </select>
        </label>
        <button class="primary" @click="opsOffset = 0; loadOps()">{{ t('history.operations.refresh') }}</button>
        <span class="hint">{{ opsMeta }}</span>
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
              <td>{{ o.state || t('common.empty') }}</td>
              <td :title="o.fingerprint || ''">{{ shortFingerprint(o.fingerprint) }}</td>
              <td :title="payloadTitle(o.payload_summary)">{{ payloadSummary(o.payload_summary) }}</td>
              <td>{{ o.actor_source || t('common.empty') }}</td>
              <td>
                <button
                  v-if="o.state === 'unknown' || o.state === 'expired'"
                  class="danger"
                  :title="t('history.operations.resolveTitle')"
                  @click="resolveStale((o.operation_id || o.id) as string, o.state)"
                >
                  {{ t('history.operations.resolve') }}
                </button>
                <span v-else class="hint">{{ t('history.operations.noAction') }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="hint">{{ t('history.operations.staleHint') }}</p>
      <div class="pager">
        <button @click="opsPrev">{{ t('history.pager.prev') }}</button>
        <span class="hint">{{ opsPageText() }}</span>
        <button @click="opsNext">{{ t('history.pager.next') }}</button>
      </div>
    </section>

    <section class="card">
      <h2>{{ t('history.audit.title') }}</h2>
      <div class="filters">
        <label>{{ t('history.audit.operationId') }}
          <input v-model="fOpId" type="text" :placeholder="t('history.audit.placeholder')" @change="auditOffset = 0" />
        </label>
        <label>{{ t('history.audit.limit') }}
          <select v-model.number="fAuditLimit" @change="auditOffset = 0; loadAudit()">
            <option :value="20">20</option>
            <option :value="50">50</option>
            <option :value="100">100</option>
          </select>
        </label>
        <button class="primary" @click="auditOffset = 0; loadAudit()">{{ t('history.audit.refresh') }}</button>
        <span class="hint">{{ auditMeta }}</span>
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
      <div class="pager">
        <button @click="auditPrev">{{ t('history.pager.prev') }}</button>
        <span class="hint">{{ auditPageText() }}</span>
        <button @click="auditNext">{{ t('history.pager.next') }}</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.stack { display: grid; gap: 14px; }
</style>
