import { createApp } from 'vue'
import App from './App.vue'
import { i18n } from './i18n'
import './style.css'
import { initThemeEarly } from './composables/useTheme'

// Ensure theme applied before mount (bootstrap also does it; this is fallback)
initThemeEarly()

createApp(App).use(i18n).mount('#app')
