// Parcours pack RDC : modèle sourcé (brouillon), personnes à charge, simulation avec réduction expliquée sur le bulletin.
const { chromium } = require('playwright')
const path = require('path')
const B = 'http://127.0.0.1:5173', OUT = path.join(__dirname, '..', 'docs', 'screenshots'), PWD = 'Demo-Passw0rd-2026'
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const login = async (p, email) => { await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill(email); await p.getByLabel('Mot de passe').fill(PWD); await p.click('role=button[name="Se connecter"]'); await p.waitForURL(B + '/') }
;(async () => {
  const b = await chromium.launch(), errors = []
  const mk = async () => { const c = await b.newContext({ viewport: { width: 1280, height: 1100 }, deviceScaleFactor: 2 }); const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message)); return p }

  // 1. Modèle RDC côté responsable paie
  const pay = await mk(); await login(pay, 'paie1@demo.test'); await pay.goto(B + '/payroll')
  await pay.waitForSelector('text=RDC — modèle 2026 (à valider)')
  await pay.locator('role=button[name="Voir le détail et les sources"]').first().click()
  await pay.waitForSelector('text=À compléter par un expert')
  assert((await pay.locator('a[href*="communityhub.sage.com"]').count()) === 1, 'sources citées avec lien')
  assert((await pay.locator('text=NON CONFIRMÉ').count()) >= 1, 'hypothèses non confirmées signalées')
  await pay.screenshot({ path: OUT + '/36-paie-modele-rdc.png', fullPage: true })
  await pay.locator('role=button[name="Créer un brouillon depuis ce modèle"]').first().click()
  await pay.waitForSelector('tr:has-text("RDC — modèle 2026") >> text=/brouillon|draft/i'); assert(true, 'brouillon créé (jamais approuvé automatiquement)')
  assert((await pay.locator('tr:has-text("RDC — modèle 2026") >> role=button[name="Approuver…"]').count()) === 1, 'approbation par une 2e personne requise')

  // 2. Personnes à charge côté RH
  const hr = await mk(); await login(hr, 'rh@demo.test'); await hr.goto(B + '/employees')
  await hr.getByRole('link', { name: /Employée/ }).first().click()
  await hr.getByRole('tab', { name: 'Rémunération' }).click()
  await hr.waitForSelector('text=Personnes fiscalement à charge')
  const form = hr.locator('form:has(input[type=number][max="30"])')
  await form.locator('input[type=number]').fill('2'); await form.locator('input[type=date]').fill('2026-01-01')
  await form.getByRole('button', { name: 'Enregistrer' }).click(); await hr.waitForSelector('td:text-is("2")')
  await hr.screenshot({ path: OUT + '/37-personnes-a-charge.png' }); assert(true, 'personnes à charge enregistrées')

  // 3. Simulation et bulletin avec explication
  await pay.reload(); await pay.waitForSelector('text=Préparer une paie')
  await pay.getByLabel('Année').fill('2030'); await pay.getByLabel('Mois').fill('3'); await pay.click('role=button[name="Créer la période"]')
  await pay.waitForTimeout(800)
  await pay.getByLabel('Période').selectOption({ label: 'mars 2030' }).catch(async () => { const o = await pay.getByLabel('Période').locator('option').allTextContents(); console.log(o); throw new Error('période') })
  await pay.getByLabel('Règles').selectOption({ index: (await pay.getByLabel('Règles').locator('option').allTextContents()).findIndex((t) => t.includes('RDC')) })
  await pay.click('role=button[name="Calculer"]'); await pay.waitForTimeout(1200)
  await pay.locator('tr:has-text("mars 2030") a, tr:has-text("mars 2030") >> role=link').first().click()
  await pay.waitForSelector('text=Détail par employé')
  assert((await pay.locator('text=Simulation : le jeu de règles n').count()) >= 1, 'simulation (règles non approuvées)')
  await pay.locator('tr:has-text("M-002")').getByRole('button', { name: 'Voir le calcul' }).click()
  await pay.waitForSelector('text=personnes à charge')
  assert(true, 'bulletin : réduction pour personnes à charge expliquée')
  await pay.screenshot({ path: OUT + '/39-bulletin-rdc.png', fullPage: true })
  await pay.screenshot({ path: OUT + '/38-paie-simulation-rdc.png', fullPage: true })
  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
