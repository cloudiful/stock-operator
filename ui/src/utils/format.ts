export function fmtTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  try {
    const d = new Date(iso)
    return d.toLocaleString()
  } catch {
    return iso
  }
}

export function shortFingerprint(fp: string | null | undefined): string {
  if (!fp) return '—'
  return fp.slice(0, 10) + '…'
}

export function payloadSummary(v: unknown): string {
  if (v === null || v === undefined) return '—'
  try {
    if (typeof v === 'string') return v.slice(0, 80)
    const s = JSON.stringify(v)
    return s.length > 100 ? s.slice(0, 100) + '…' : s
  } catch {
    return String(v).slice(0, 80)
  }
}

export function payloadTitle(v: unknown): string {
  if (v === null || v === undefined) return ''
  try {
    if (typeof v === 'string') return v
    return JSON.stringify(v)
  } catch {
    return String(v)
  }
}
