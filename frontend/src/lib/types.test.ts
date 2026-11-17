import { describe, expect, it } from 'vitest'
import { needsMfa, type LoginResult } from './types'

describe('needsMfa', () => {
  it('détecte le défi MFA', () => {
    expect(needsMfa({ mfa_required: true, mfa_token: 't' })).toBe(true)
  })
  it('ne confond pas un profil avec un défi', () => {
    const me = { user: { id: '1' }, permissions: [] } as unknown as LoginResult
    expect(needsMfa(me)).toBe(false)
  })
})
