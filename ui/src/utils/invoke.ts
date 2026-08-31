export function isTauri(): boolean {
  return !!(window as unknown as { __TAURI__?: { core?: { invoke?: unknown } } }).__TAURI__?.core?.invoke
}

export function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const invoke = (window as unknown as { __TAURI__?: { core: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<T> } } }).__TAURI__?.core?.invoke
  if (invoke) return invoke(cmd, args ?? {})
  return Promise.reject(new Error('Tauri not available – open the desktop app'))
}

export type InvokeFn = typeof tauriInvoke
