// Parcours MFA obligatoire : l'admin impose la règle, un membre sans MFA est redirigé vers « Sécurité » puis débloqué.
const { chromium } = require('playwright')
const crypto = require('crypto'), path = require('path')
const B = 'http://127.0.0.1:5173'
const OUT = path.join(__dirname, '..', 'docs', 'screenshots')
const PWD = 'Demo-Passw0rd-2026'
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const b32 = (s) => { let bits = 0, v = 0; const o = []; for (const ch of s) { v = (v << 5) | 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'.indexOf(ch); bits += 5; if (bits >= 8) { o.push((v >>> (bits - 8)) & 255); bits -= 8 } } return Buffer.from(o) }
const totp = (secret, off = 0) => {
  const step = Math.floor(Date.now() / 30000) + off
  const buf = Buffer.alloc(8); buf.writeBigUInt64BE(BigInt(step))
  const h = crypto.createHmac('sha1', b32(secret)).update(buf).digest(), o = h[19] & 15
  return String(((h.readUInt32BE(o) & 0x7fffffff) % 1e6)).padStart(6, '0')
}
const login = async (p, email) => { await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill(email); await p.getByLabel('Mot de passe').fill(PWD); await p.click('role=button[name="Se connecter"]'); await p.waitForURL(B + '/') }
const enable = async (p) => {
  await p.click('role=link[name="Sécurité"]'); await p.waitForSelector('text=Désactivée')
  await p.getByLabel('Mot de passe actuel').fill(PWD); await p.click('role=button[name="Configurer le MFA"]')
  await p.waitForSelector('role=img[name="QR code de configuration"]')
  const secret = (await p.locator('p.font-mono').first().textContent()).trim()
  await p.getByLabel('Code à 6 chiffres').fill(totp(secret)); await p.click('role=button[name="Activer"]')
  await p.waitForSelector('[data-testid=recovery-codes]'); await p.click('role=button[name="J\'ai enregistré mes codes"]')
}
;(async () => {
  const b = await chromium.launch(), errors = []
  const mk = async () => { const c = await b.newContext({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 }); const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message)); return p }
  const hr = await mk(); await login(hr, 'rh@demo.test'); await enable(hr)
  await hr.waitForSelector('text=Politique de la société')
  await hr.getByLabel(/Exiger l'authentification à deux facteurs/).click()
  await hr.waitForFunction(() => document.querySelector('input[type=checkbox]').checked); assert(true, 'politique activée par l\'admin')
  const pay = await mk(); await login(pay, 'paie1@demo.test')
  await pay.goto(B + '/payroll'); await pay.waitForURL(B + '/security')
  await pay.waitForSelector('text=Votre société exige'); assert(true, 'membre sans MFA redirigé vers Sécurité')
  await pay.screenshot({ path: OUT + '/45-mfa-obligatoire.png' })
  await pay.goto(B + '/'); await pay.waitForURL(B + '/security'); assert(true, 'aucune autre page accessible')
  await pay.reload(); await enable(pay)
  await pay.goto(B + '/payroll'); await pay.waitForSelector('text=Périodes', { timeout: 8000 }).catch(() => {})
  assert(pay.url() === B + '/payroll', 'accès rétabli après activation du MFA')
  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
