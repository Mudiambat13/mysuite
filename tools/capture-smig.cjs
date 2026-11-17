// Parcours grille SMIG : import (vérification puis import), classe d'un employé, rapport de conformité.
const { chromium } = require('playwright')
const path = require('path')
const B = 'http://127.0.0.1:5173', OUT = path.join(__dirname, '..', 'docs', 'screenshots'), PWD = 'Demo-Passw0rd-2026'
const CSV = process.argv[2]
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
const login = async (p, email) => { await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill(email); await p.getByLabel('Mot de passe').fill(PWD); await p.click('role=button[name="Se connecter"]'); await p.waitForURL(B + '/') }
;(async () => {
  const b = await chromium.launch(), errors = []
  const mk = async () => { const c = await b.newContext({ viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 2 }); const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message)); return p }
  const hr = await mk(); await login(hr, 'rh@demo.test'); await hr.goto(B + '/employees')
  await hr.getByRole('link', { name: /Employée/ }).first().click()
  await hr.getByRole('tab', { name: 'Rémunération' }).click()
  await hr.waitForSelector('text=Classe de la grille SMIG')
  const cf = hr.locator('form:has(input[type=number][min="1"]:not([max]))')
  await cf.locator('input[type=number]').fill('1'); await cf.locator('input[type=date]').fill('2026-01-01')
  await cf.getByRole('button', { name: 'Enregistrer' }).click(); await hr.waitForSelector('td:text-is("1")')
  assert(true, 'classe affectée à un employé')

  const pay = await mk(); await login(pay, 'paie1@demo.test'); await pay.goto(B + '/payroll')
  await pay.getByRole('link', { name: 'Grille SMIG et conformité' }).click()
  await pay.waitForSelector('text=Importer une grille')
  await pay.getByLabel('Nom de la grille').fill('SMIG RDC 2025')
  await pay.getByLabel('En vigueur au').fill('2025-06-01')
  await pay.getByLabel('Source officielle').fill('Décret n° 25/22 du 30 mai 2025, annexe 2 (à confirmer au Journal officiel)')
  await pay.locator('input[type=file]').setInputFiles(CSV)
  await pay.getByRole('button', { name: 'Vérifier sans importer' }).click()
  await pay.waitForSelector('text=lignes valides')
  assert((await pay.locator('text=17 lignes valides').count()) === 1, '17 classes lues dans le fichier')
  await pay.screenshot({ path: OUT + '/40-smig-import.png', fullPage: true })
  await pay.getByRole('button', { name: 'Importer', exact: true }).click()
  await pay.waitForSelector('td:text-is("SMIG RDC 2025")'); assert(true, 'grille importée')
  await pay.waitForSelector('text=Sous le minimum')
  assert((await pay.locator('text=Classe manquante').count()) >= 1, 'employés sans classe signalés, non bloqués')
  await pay.screenshot({ path: OUT + '/41-smig-conformite.png', fullPage: true })
  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
