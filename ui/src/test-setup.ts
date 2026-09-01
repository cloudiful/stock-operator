// Ensure localStorage and related browser globals are available in jsdom + Bun runtime.
// Bun provides a global localStorage stub that requires --localstorage-file; jsdom provides window.localStorage
// but may be opaque origin without url. Provide an in-memory polyfill when needed.
function createMemoryStorage(): Storage {
  let store = new Map<string, string>()
  return {
    get length() {
      return store.size
    },
    clear() {
      store.clear()
    },
    getItem(key: string) {
      return store.get(key) ?? null
    },
    key(index: number) {
      return Array.from(store.keys())[index] ?? null
    },
    removeItem(key: string) {
      store.delete(key)
    },
    setItem(key: string, value: string) {
      store.set(key, String(value))
    },
  } as unknown as Storage
}

if (typeof window !== 'undefined') {
  // @ts-ignore
  if (!window.localStorage || typeof window.localStorage.getItem !== 'function') {
    // @ts-ignore
    try {
      // jsdom throws on access when opaque origin
      void window.localStorage
    } catch {}
    // Polyfill
    // @ts-ignore
    window.localStorage = createMemoryStorage()
  }
  // Also ensure globalThis sees same instance
  // @ts-ignore
  try {
    if (!globalThis.localStorage || typeof (globalThis as unknown as { localStorage?: Storage }).localStorage?.getItem !== 'function') {
      // @ts-ignore
      globalThis.localStorage = window.localStorage
    }
  } catch {
    // @ts-ignore
    globalThis.localStorage = window.localStorage
  }
  // @ts-ignore
  if (typeof globalThis.sessionStorage === 'undefined' || !window.sessionStorage) {
    // @ts-ignore
    try {
      if (window.sessionStorage) globalThis.sessionStorage = window.sessionStorage
    } catch {}
    if (!globalThis.sessionStorage) {
      // @ts-ignore
      globalThis.sessionStorage = createMemoryStorage()
      // @ts-ignore
      window.sessionStorage = globalThis.sessionStorage
    }
  }
  // Ensure document and navigator are on globalThis for bare references
  // @ts-ignore
  if (typeof globalThis.document === 'undefined') globalThis.document = window.document
  // @ts-ignore
  if (typeof globalThis.navigator === 'undefined') globalThis.navigator = window.navigator
} else {
  // No window (should not happen in jsdom), polyfill global
  // @ts-ignore
  if (typeof globalThis.localStorage === 'undefined') globalThis.localStorage = createMemoryStorage()
}
