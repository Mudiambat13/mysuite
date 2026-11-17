// Parcours badgeuse : clé d'appareil, envoi API, badges, import CSV (simulation puis réel), journal + captures.
const { chromium } = require('playwright')
const fs = require('fs'), os = require('os'), path = require('path')
const B = 'http://127.0.0.1:5173'
const OUT = path.join(__dirname, '..', 'docs', 'screenshots')
const H = { 'X-Requested-With': 'mysuite', 'Content-Type': 'application/json' }
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const iso = (d) => d.toISOString().replace(/\.\d{3}Z$/, 'Z')
;(async () => {
  const b = await chromium.launch()
  const c = await b.newContext({ viewport: { width: 1440, height: 1500 }, deviceScaleFactor: 2 })
  const r = await c.request.post(B + '/api/v1/auth/login', { headers: H, data: { email: 'rh@demo.test', password: 'Demo-Passw0rd-2026' } })
  assert(r.ok(), 'connexion RH')
  const errors = []
  const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message))
  await p.goto(B + '/attendance'); await p.click('role=tab[name="Badgeuses"]'); await p.waitForSelector('text=Badgeuses connectées')

  // 1. Appareil : la clé n'apparaît qu'une fois
  await p.getByLabel('Nouvelle badgeuse').fill('Entrée principale')
  await p.click('role=button[name="Générer une clé"]'); await p.waitForSelector('[data-testid=api-key]')
  const key = (await p.textContent('[data-testid=api-key]')).trim()
  assert(key.startsWith('mskey_'), 'clé affichée dans la fenêtre')
  await p.screenshot({ path: OUT + '/27-badgeuse-cle.png' })
  await p.click('role=button[name="J’ai copié la clé"]')
  assert((await p.locator('text=' + key).count()) === 0, 'la clé n’est plus affichée après fermeture')

  // 2. Envoi API par la « badgeuse » (Bearer, sans cookie ni en-tête CSRF)
  const emps = await (await c.request.get(B + '/api/v1/employees?limit=100&status=active')).json()
  const [e1, e2, e3] = emps.items.filter((e) => e.matricule !== 'M-005').slice(0, 3)
  const now = Date.now(), h = 3600e3
  const raw = await b.newContext() // aucun cookie
  const send = (punches) => raw.request.post(B + '/api/v1/device/punches', { headers: { Authorization: 'Bearer ' + key, 'Content-Type': 'application/json' }, data: { punches } })
  const a = await (await send([
    { badge: e1.matricule, at: iso(new Date(now - 600 * h)) },
    { badge: e1.matricule, at: iso(new Date(now - 592 * h)) },
    { badge: 'A1B2C3', at: iso(new Date(now - 599 * h)) },
  ])).json()
  console.log(JSON.stringify(a))
  assert(a.counts.entries_opened === 1 && a.counts.entries_closed === 1 && a.counts.unknown_badges === 1, 'envoi appareil : 1 entrée, 1 sortie, 1 badge inconnu')
  const again = await (await send([{ badge: e1.matricule, at: iso(new Date(now - 600 * h)) }])).json()
  assert(again.counts.duplicates === 1, 'renvoi identique ignoré (idempotent)')
  assert((await raw.request.post(B + '/api/v1/device/punches', { headers: { Authorization: 'Bearer mskey_faux' }, data: { punches: [] } })).status() === 401, 'clé invalide : 401')

  // 3. Journal : le badge inconnu est visible ; on l'associe puis on retraite
  await p.reload(); await p.click('role=tab[name="Badgeuses"]'); await p.waitForSelector('td:has-text("A1B2C3")')
  await p.selectOption('role=combobox[name="Employé"]', e3.id)
  await p.getByLabel('Code du badge').fill('A1B2C3')
  await p.click('role=button[name="Associer"]'); await p.waitForSelector('td.font-mono:has-text("A1B2C3")')
  await p.click('role=button[name="Retraiter les inconnus/refusés"]'); await p.waitForSelector('text=Retraitement :')
  assert(await p.locator('text=Retraitement : 1 passage').count() === 1, 'retraitement : le passage inconnu est appliqué')

  // 4. Import CSV : simulation puis import réel
  const d = new Date(now - 28 * 24 * h)
  const ymd = d.toISOString().slice(0, 10)
  const csv = `Badge;Date_Heure;Sens\n${e1.matricule};${ymd} 08:02;entrée\n${e1.matricule};${ymd} 17:10;sortie\n${e2.matricule};${ymd} 07:58;\n${e2.matricule};${ymd} 16:00;\nXX-404;${ymd} 09:00;\n${e1.matricule};date illisible;\n`
  const f = path.join(os.tmpdir(), 'export-badgeuse.csv'); fs.writeFileSync(f, csv)
  await p.setInputFiles('input[type=file]', f)
  await p.click('role=button[name="Simuler"]'); await p.waitForSelector('text=Simulation —')
  assert(await p.locator('text=ligne 7').count() > 0, 'simulation : erreur signalée avec numéro de ligne')
  const before = await (await c.request.get(B + '/api/v1/attendance/entries?scope=all&from=' + ymd + '&to=' + ymd)).json()
  assert(before.length === 0, 'simulation : rien n’est enregistré')
  await p.screenshot({ path: OUT + '/28-badgeuse-import-simulation.png' })
  await p.click('role=button[name="Importer"]'); await p.waitForSelector('text=Import terminé.')
  const after = await (await c.request.get(B + '/api/v1/attendance/entries?scope=all&from=' + ymd + '&to=' + ymd)).json()
  console.log(JSON.stringify(after.map((e) => [e.matricule, e.clock_in, e.clock_out, e.source])))
  assert(after.length === 2 && after.every((e) => e.source === 'import'), 'import réel : 2 pointages créés')
  const t1 = after.find((e) => e.matricule === e1.matricule)
  assert(t1.clock_in.slice(11, 16) === '06:02', '08:02 locale (UTC+2) enregistrée à 06:02Z')
  await p.waitForTimeout(500)
  await p.screenshot({ path: OUT + '/29-badgeuse-journal.png', fullPage: true })
  console.log('erreurs page:', errors.length ? errors.slice(0, 3) : 'aucune')
  await b.close()
})().catch((e) => { console.log('FAIL', e.message.slice(0, 600)); process.exit(1) })
