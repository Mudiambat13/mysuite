// Parcours sécurité : activation MFA (QR + codes de secours), connexion en 2 étapes, rejeu, désactivation,
// réinitialisation admin du MFA et lien de réinitialisation de mot de passe + captures.
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
const login = async (p, email, pwd = PWD) => { await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill(email); await p.getByLabel('Mot de passe').fill(pwd); await p.click('role=button[name="Se connecter"]') }
;(async () => {
  const b = await chromium.launch()
  const errors = []
  const ctx = async () => { const c = await b.newContext({ viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 2 }); const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message)); return [c, p] }

  // 1. Activation par l'employé
  const [, e] = await ctx()
  await login(e, 'employe@demo.test'); await e.waitForURL(B + '/')
  await e.click('role=link[name="Sécurité"]'); await e.waitForSelector('text=Désactivée')
  await e.getByLabel('Mot de passe actuel').fill('mauvais'); await e.click('role=button[name="Configurer le MFA"]')
  await e.locator('[role=alert]').first().waitFor()
  await e.getByLabel('Mot de passe actuel').fill(PWD); await e.click('role=button[name="Configurer le MFA"]')
  await e.waitForSelector('role=img[name="QR code de configuration"]')
  const secret = (await e.locator('p.font-mono').first().textContent()).trim()
  assert(/^[A-Z2-7]{32}$/.test(secret), 'secret base32 affiché')
  await e.screenshot({ path: OUT + '/30-securite-activation.png' })
  await e.getByLabel('Code à 6 chiffres').fill('000000'); await e.click('role=button[name="Activer"]')
  await e.waitForSelector('text=incorrect ou expiré')
  await e.getByLabel('Code à 6 chiffres').fill(totp(secret)); await e.click('role=button[name="Activer"]')
  await e.waitForSelector('[data-testid=recovery-codes]')
  const codes = (await e.locator('[data-testid=recovery-codes] li').allTextContents()).map((s) => s.trim())
  assert(codes.length === 10, '10 codes de secours affichés une fois')
  await e.screenshot({ path: OUT + '/31-securite-codes-secours.png' })
  await e.click('role=button[name="J\'ai enregistré mes codes"]')
  await e.waitForSelector('text=Activée'); assert((await e.locator('[data-testid=recovery-codes]').count()) === 0, 'codes masqués ensuite')

  // 2. Connexion en 2 étapes
  const [, l] = await ctx()
  await login(l, 'employe@demo.test'); await l.waitForSelector('text=application d\'authentification')
  assert(l.url().endsWith('/login'), 'pas de session après le mot de passe seul')
  await l.screenshot({ path: OUT + '/32-connexion-mfa.png' })
  await l.getByLabel('Code de vérification').fill('111111'); await l.click('role=button[name="Vérifier"]')
  await l.waitForSelector('text=/identifiants|invalide|incorrect|authentif/i')
  await l.getByLabel('Code de vérification').fill(totp(secret, 1)); await l.click('role=button[name="Vérifier"]')
  await l.waitForURL(B + '/'); assert(true, 'connexion avec code TOTP')
  // Code de secours (nouvelle connexion)
  const [, l2] = await ctx()
  await login(l2, 'employe@demo.test'); await l2.getByLabel('Code de vérification').fill(codes[0]); await l2.click('role=button[name="Vérifier"]')
  await l2.waitForURL(B + '/'); assert(true, 'connexion avec un code de secours')
  const [, l3] = await ctx()
  await login(l3, 'employe@demo.test'); await l3.getByLabel('Code de vérification').fill(codes[0]); await l3.click('role=button[name="Vérifier"]')
  await l3.waitForSelector('text=/identifiants|invalide|incorrect|authentif/i'); assert(l3.url().endsWith('/login'), 'code de secours déjà utilisé : refusé')

  // 3. Réinitialisation par l'administrateur RH (MFA + lien)
  const [, hr] = await ctx()
  await login(hr, 'rh@demo.test'); await hr.waitForURL(B + '/'); await hr.goto(B + '/users')
  await hr.waitForSelector('text=Comptes')
  const row = hr.locator('tr', { hasText: 'employe@demo.test' })
  await row.getByRole('button', { name: /Lien de réinitialisation/ }).click()
  await hr.waitForSelector('[data-testid=reset-link]')
  const link = (await hr.textContent('[data-testid=reset-link]')).trim()
  assert(link.includes('/reset-password?token='), 'lien affiché une fois')
  await hr.screenshot({ path: OUT + '/33-lien-reinitialisation.png' })
  await hr.click('role=button[name="Fermer"]')
  hr.once('dialog', (d) => d.accept())
  await row.getByRole('button', { name: /Réinitialiser le MFA/ }).click()
  await hr.waitForSelector('text=MFA retiré')
  await hr.screenshot({ path: OUT + '/34-utilisateurs-acces.png' })

  // 4. L'employé utilise le lien
  const [, r] = await ctx()
  await r.goto(link); await r.getByLabel('Nouveau mot de passe').fill('Nouveau-Passw0rd-2026'); await r.getByLabel('Confirmer').fill('Nouveau-Passw0rd-2026')
  await r.click('role=button[name="Enregistrer"]'); await r.waitForSelector('text=Mot de passe modifié')
  await r.screenshot({ path: OUT + '/35-nouveau-mot-de-passe.png' })
  await r.goto(link); await r.getByLabel('Nouveau mot de passe').fill('Autre-Passw0rd-2026'); await r.getByLabel('Confirmer').fill('Autre-Passw0rd-2026')
  await r.click('role=button[name="Enregistrer"]'); await r.waitForSelector('text=lien invalide ou expiré'); assert(true, 'lien à usage unique')
  const [, n] = await ctx()
  await login(n, 'employe@demo.test', 'Nouveau-Passw0rd-2026'); await n.waitForURL(B + '/'); assert(true, 'connexion avec le nouveau mot de passe, sans MFA (réinitialisé)')

  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
