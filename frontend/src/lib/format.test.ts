import { describe, expect, it } from 'vitest'
import { money, periodLabel } from './format'
import { qs } from './api'

describe('format', () => {
  it('affiche un tiret pour une valeur absente', () => {
    expect(money(null)).toBe('—')
  })
  it('formate une période', () => {
    expect(periodLabel(2026, 2)).toBe('février 2026')
  })
  it('construit une query string sans valeurs vides', () => {
    expect(qs({ a: 1, b: '', c: undefined, d: 'x y' })).toBe('?a=1&d=x+y')
  })
})

import { hoursMinutes, isoToZonedInput, zonedInputToIso } from './format'

describe('pointage : heures et fuseaux', () => {
  it('formate les minutes en heures', () => {
    expect(hoursMinutes(485)).toBe('8 h 05')
    expect(hoursMinutes(null)).toBe('—')
  })
  it('convertit dans le fuseau de la société (UTC+2, sans heure d’été)', () => {
    expect(isoToZonedInput('2026-03-02T06:25:00Z', 'Africa/Lubumbashi')).toBe('2026-03-02T08:25')
    expect(zonedInputToIso('2026-03-02T08:25', 'Africa/Lubumbashi')).toBe('2026-03-02T06:25:00.000Z')
  })
  it('gère l’heure d’été (Paris)', () => {
    expect(zonedInputToIso('2026-07-01T09:00', 'Europe/Paris')).toBe('2026-07-01T07:00:00.000Z')
    expect(zonedInputToIso('2026-01-15T09:00', 'Europe/Paris')).toBe('2026-01-15T08:00:00.000Z')
  })
})
