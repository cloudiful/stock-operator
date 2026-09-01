<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

export type AppPage = 'connection' | 'history' | 'settings'

const props = defineProps<{ activePage: AppPage }>()
const emit = defineEmits<{ (e: 'update:activePage', v: AppPage): void }>()

const { t } = useI18n()

const items = computed(() => [
  { id: 'connection' as const, label: t('sidebar.connection'), icon: 'i-lucide-activity', desc: t('sidebar.connectionDesc') },
  { id: 'history' as const, label: t('sidebar.history'), icon: 'i-lucide-history', desc: t('sidebar.historyDesc') },
  { id: 'settings' as const, label: t('sidebar.settings'), icon: 'i-lucide-settings-2', desc: t('sidebar.settingsDesc') },
])

function select(page: AppPage) {
  emit('update:activePage', page)
}
</script>

<template>
  <aside class="app-sidebar" aria-label="Primary">
    <div class="sidebar-brand">
      <span class="brand-icon" aria-hidden="true">◈</span>
      <span class="brand-text">{{ t('common.brand') }}</span>
    </div>
    <nav class="sidebar-nav" aria-label="Main navigation">
      <ul class="nav-list" role="list">
        <li v-for="item in items" :key="item.id" class="nav-item">
          <UButton
            :color="props.activePage === item.id ? 'primary' : 'neutral'"
            :variant="props.activePage === item.id ? 'solid' : 'ghost'"
            :icon="item.icon"
            block
            :aria-current="props.activePage === item.id ? 'page' : undefined"
            :title="item.desc"
            class="justify-start"
            @click="select(item.id)"
          >
            {{ item.label }}
          </UButton>
        </li>
      </ul>
    </nav>
    <div class="sidebar-footer">
      <span class="text-xs text-muted">{{ t('sidebar.desktopHint') }}</span>
    </div>
  </aside>
</template>

<style scoped>
.app-sidebar {
  width: 220px;
  flex-shrink: 0;
  background: var(--card);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 16px 12px;
  gap: 16px;
  min-height: 100vh;
}
.sidebar-brand {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 700;
  font-size: 15px;
  letter-spacing: -0.01em;
  padding: 4px 8px;
}
.brand-icon { color: var(--primary); font-size: 16px; }
.sidebar-nav { flex: 1; }
.nav-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.nav-button {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--muted);
  font-weight: 600;
  font-size: 13px;
  cursor: pointer;
  text-align: left;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}
.nav-button:hover { background: var(--table-head-bg); color: var(--text); border-color: var(--border); }
.nav-button:focus-visible { outline: 2px solid var(--primary); outline-offset: 2px; }
.nav-button.active {
  background: var(--primary);
  color: var(--primary-fg);
  border-color: var(--primary);
}
.nav-button.active .nav-icon { color: var(--primary-fg); }
.nav-icon { width: 16px; height: 16px; display: inline-block; }
.sidebar-footer { padding: 8px; border-top: 1px solid var(--border); margin-top: auto; }

@media (max-width: 960px) {
  .app-sidebar {
    width: 100%;
    min-height: auto;
    flex-direction: row;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-right: none;
    border-bottom: 1px solid var(--border);
  }
  .sidebar-brand { flex-shrink: 0; margin-right: 8px; }
  .sidebar-nav { flex: 1; }
  .nav-list { flex-direction: row; gap: 8px; }
  .nav-button { padding: 6px 12px; border-radius: 999px; }
  .sidebar-footer { display: none; }
}
</style>
