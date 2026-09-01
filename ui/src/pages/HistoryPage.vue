<script setup lang="ts">
import HistoryPanel from '@/components/HistoryPanel.vue'

const emit = defineEmits<{ (e: 'message', text: string, kind: 'ok' | 'err' | 'info'): void }>()

function onMessage(text: string, kind: 'ok' | 'err' | 'info') {
  emit('message', text, kind)
}
</script>

<template>
  <div class="space-y-4">
    <div>
      <h1 id="page-title-history" class="text-lg font-semibold tracking-tight">{{ $t('history.operations.title') }} & {{ $t('history.audit.title') }}</h1>
      <p class="text-sm text-muted">{{ $t('history.operations.staleHint') }}</p>
    </div>
    <HistoryPanel @message="onMessage" />
  </div>
</template>
