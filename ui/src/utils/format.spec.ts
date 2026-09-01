import { describe, it, expect } from 'vitest'
import { fmtTime, shortFingerprint, payloadSummary } from './format'

describe('format utils', () => {
  it('fmtTime returns — for null', () => {
    expect(fmtTime(null)).toBe('—')
    expect(fmtTime(undefined)).toBe('—')
  })

  it('fmtTime returns locale string for valid iso', () => {
    const iso = '2024-01-02T03:04:05.000Z'
    const out = fmtTime(iso)
    expect(out).not.toBe('—')
    expect(out.length).toBeGreaterThan(0)
  })

  it('shortFingerprint truncates', () => {
    expect(shortFingerprint('abcdef1234567890')).toBe('abcdef1234…')
    expect(shortFingerprint(null)).toBe('—')
    expect(shortFingerprint('')).toBe('—')
  })

  it('payloadSummary stringifies and truncates', () => {
    expect(payloadSummary(null)).toBe('—')
    expect(payloadSummary('hello world')).toBe('hello world')
    const long = 'a'.repeat(200)
    expect(payloadSummary(long).length).toBe(80)
    const obj = { a: 1, b: 'x'.repeat(200) }
    const summary = payloadSummary(obj)
    expect(summary.length).toBeLessThanOrEqual(101)
  })

  it('payloadSummary escapes via text not html', () => {
    const evil = '<script>alert(1)</script>'
    const out = payloadSummary(evil)
    expect(out).toBe(evil.slice(0, 80))
    // ensures caller would use textContent, not v-html
    expect(out).toContain('<script>')
  })
})
