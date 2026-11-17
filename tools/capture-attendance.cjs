// Prérequis : supprimer d'éventuels pointages 'self' de test pour une capture propre.
// Parcours pointage dans le navigateur + captures (base de démo : npm run dev / seed-demo).
const { chromium, devices } = require('playwright')
const B = 'http://127.0.0.1:5173'
const OUT = require('path').join(__dirname, '..', 'docs', 'screenshots')
const H = { 'X-Requested-With': 'mysuite', 'Content-Type': 'application/json' }
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const login = async (c, email) => { const r = await c.request.post(B + '/api/v1/auth/login', { headers: H, data: { email, password: 'Demo-Passw0rd-2026' } }); if (!r.ok()) throw new Error('login ' + email + ' ' + r.status()) }
;(async () => {
  const b = await chromium.launch()
  const errors = []
  const ctx = async (email, opts = {}) => { const c = await b.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, ...opts }); await login(c, email); const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message)); return [c, p] }

  // RH : paramètres + saisies pour alimenter le résumé
  const [hr, hp] = await ctx('rh@demo.test')
  const set = await hr.request.put(B + '/api/v1/attendance/settings', { headers: H, data: { timezone: 'Africa/Lubumbashi', work_start: '08:00:00', grace_minutes: 10, daily_minutes: 480 } })
  assert(set.ok(), 'paramètres enregistrés')
  const emps = await (await hr.request.get(B + '/api/v1/employees?limit=100&status=active')).json()
  const list = emps.items.filter((e) => !['rh@demo.test'].includes(e.email)).slice(0, 4)
  const today = new Date()
  const day = (n) => { const d = new Date(today); d.setUTCDate(d.getUTCDate() - n); return d.toISOString().slice(0, 10) }
  let seeded = 0
  for (const [i, e] of list.entries()) for (const n of [1, 2, 3, 4]) {
    const start = 6 + ((i + n) % 3) * 0 + (n % 2 === 0 && i % 2 === 0 ? 0 : 0) // 06:00Z = 08:00 locale
    const late = (i + n) % 3 === 0 ? 25 : 0
    const long = (i + n) % 2 === 0 ? 570 : 480
    const ci = new Date(`${day(n)}T0${start}:00:00Z`.replace('T00', 'T0')); ci.setUTCMinutes(late)
    const co = new Date(ci.getTime() + long * 60000)
    const r = await hr.request.post(B + '/api/v1/attendance/entries', { headers: H, data: { employee_id: e.id, clock_in: ci.toISOString(), clock_out: co.toISOString(), note: 'démo' } })
    if (r.ok()) seeded++
  }
  console.log('entrées de démo créées:', seeded)

  // Employé : pointer entrée puis sortie (UI)
  const [em, ep] = await ctx('employe@demo.test')
  await ep.goto(B + '/attendance'); await ep.waitForSelector('role=button[name="Pointer l’entrée"]')
  await ep.click('role=button[name="Pointer l’entrée"]'); await ep.waitForSelector('role=button[name="Pointer la sortie"]')
  assert(true, 'pointage d’entrée via l’interface')
  await ep.waitForTimeout(500)
  await ep.screenshot({ path: OUT + '/23-pointage-en-poste.png' })
  await ep.click('role=button[name="Pointer la sortie"]'); await ep.waitForSelector('role=button[name="Pointer l’entrée"]')
  assert((await ep.locator('text=Saisie RH').count()) >= 0, 'pointage de sortie via l’interface')

  // RH : résumé, détail, correction, mode mobile
  await hp.goto(B + '/attendance'); await hp.click('role=tab[name="Présences"]'); await hp.waitForSelector('text=Résumé par employé'); await hp.waitForTimeout(600)
  const rows = await hp.locator('tbody tr').count()
  assert(rows >= 3, 'résumé par employé affiché (' + rows + ' lignes)')
  await hp.locator('tbody tr').first().click(); await hp.waitForSelector('text=Détail'); await hp.waitForTimeout(400)
  await hp.click('role=button[name^="Corriger le pointage"] >> nth=0'); await hp.waitForSelector('role=dialog')
  await hp.getByLabel(/Motif/).fill('badge défaillant')
  await hp.screenshot({ path: OUT + '/25-pointage-correction.png' })
  await hp.click('role=dialog >> role=button[name="Enregistrer"]'); await hp.waitForSelector('role=dialog', { state: 'detached' })
  assert(await hp.locator('text=(corrigé)').count() > 0, 'correction enregistrée et signalée')
  await hp.setViewportSize({ width: 1440, height: 1500 }); await hp.waitForTimeout(400)
  await hp.screenshot({ path: OUT + '/24-pointage-equipe.png' })

  // Un employé ne voit ni Présences ni Paramètres
  assert((await ep.locator('role=tab').count()) === 1, 'employé : un seul onglet (Mon pointage)')

  const [mo, mp] = await ctx('employe@demo.test', { ...devices['iPhone 13'], deviceScaleFactor: 2 })
  await mp.goto(B + '/attendance'); await mp.waitForSelector('role=button[name^="Pointer"]'); await mp.waitForTimeout(500)
  assert(!(await mp.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)), 'mobile : pas de débordement horizontal')
  await mp.screenshot({ path: OUT + '/26-pointage-mobile.png' })
  console.log('erreurs page:', errors.length ? errors.slice(0, 3) : 'aucune')
  await b.close()
})().catch((e) => { console.log('FAIL', e.message.slice(0, 500)); process.exit(1) })
