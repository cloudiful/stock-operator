import { createApp } from 'vue'
import ui from '@nuxt/ui/vue-plugin'
import App from './App.vue'
import { i18n } from './i18n'
import './style.css'
import { initThemeEarly } from './composables/useTheme'

// Ensure theme applied before mount (bootstrap also does it; this is fallback)
initThemeEarly()

const app = createApp(App)
app.use(i18n)
app.use(ui)
app.mount('#app')
