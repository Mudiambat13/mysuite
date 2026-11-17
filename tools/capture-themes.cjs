// Vérifie le sélecteur de thème (clair/sombre/système) et capture les graphiques en sombre.
const { chromium } = require('playwright')
const B = 'http://127.0.0.1:5173'
const OUT = require('path').join(__dirname, '..', 'docs', 'screenshots')
const H = { 'X-Requested-With': 'mysuite', 'Content-Type': 'application/json' }
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const isDark = (p) => p.evaluate(() => document.documentElement.classList.contains('dark'))
;(async () => {
  const b = await chromium.launch()
  // OS en clair : la page de connexion propose le sélecteur
  const c = await b.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: 'light' })
  const p = await c.newPage()
  await p.goto(B + '/login'); await p.waitForSelector('role=radiogroup[name="Thème"]')
  assert(!(await isDark(p)), 'système (OS clair) => clair')
  await p.click('role=radio[name="Sombre"]'); assert(await isDark(p), 'sombre sur la page de connexion')
  await p.reload(); await p.waitForSelector('text=Se connecter'); assert(await isDark(p), 'choix mémorisé après rechargement')
  await p.click('role=radio[name="Clair"]'); assert(!(await isDark(p)), 'clair')
  await p.click('role=radio[name="Système"]')
  await p.emulateMedia({ colorScheme: 'dark' }); await p.waitForTimeout(300)
  assert(await isDark(p), 'système suit l’OS en direct (passage au sombre)')
  await p.emulateMedia({ colorScheme: 'light' }); await p.waitForTimeout(300)
  assert(!(await isDark(p)), 'système suit l’OS en direct (retour au clair)')
  // connecté : graphiques en sombre
  await c.request.post(B + '/api/v1/auth/login', { headers: H, data: { email: 'rh@demo.test', password: 'Demo-Passw0rd-2026' } })
  await p.goto(B + '/reports'); await p.waitForSelector('.recharts-bar-rectangle path')
  await p.click('role=radio[name="Sombre"]'); await p.waitForTimeout(400)
  const box = await p.locator('.recharts-bar-rectangle path').first().boundingBox()
  await p.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await p.waitForTimeout(400)
  await p.screenshot({ path: OUT + '/21-rapports-sombre.png', fullPage: true })
  await p.goto(B + '/'); await p.waitForSelector('text=Répartition de l\'effectif'); await p.waitForTimeout(800)
  await p.screenshot({ path: OUT + '/22-tableau-de-bord-sombre.png' })
  await p.goto(B + '/login').catch(() => {})
  await b.close()
})().catch((e) => { console.log('FAIL', e.message.slice(0, 400)); process.exit(1) })
