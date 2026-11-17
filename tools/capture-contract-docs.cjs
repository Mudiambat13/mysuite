// Parcours modèles de contrat : création d'un modèle, génération du PDF d'un contrat.
const { chromium } = require('playwright')
const path = require('path')
const B = 'http://127.0.0.1:5173', OUT = path.join(__dirname, '..', 'docs', 'screenshots'), PWD = 'Demo-Passw0rd-2026'
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
;(async () => {
  const b = await chromium.launch(), errors = []
  const c = await b.newContext({ viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 2, acceptDownloads: true })
  const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message))
  await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill('rh@demo.test'); await p.getByLabel('Mot de passe').fill(PWD)
  await p.click('role=button[name="Se connecter"]'); await p.waitForURL(B + '/')
  await p.goto(B + '/contracts'); await p.waitForSelector('text=Modèles de contrat')
  await p.getByLabel('Nom du modèle').fill('Contrat type (exemple fictif)')
  await p.getByLabel('Texte du contrat').fill('CONTRAT DE TRAVAIL (TEXTE D\'EXEMPLE, À REMPLACER PAR VOTRE MODÈLE VALIDÉ)\n\nEntre {{company}} et {{employee_name}} (matricule {{matricule}}).\nPoste : {{job_title}}. Type : {{contract_type}}.\nDébut : {{start_date}}. Fin : {{end_date}}.')
  await p.getByRole('button', { name: 'Enregistrer le modèle' }).click()
  await p.waitForSelector('td:text-is("Contrat type (exemple fictif)")'); assert(true, 'modèle enregistré')
  await p.screenshot({ path: OUT + '/42-modeles-contrat.png', fullPage: true })
  const sel = p.locator('select[aria-label^="Générer le PDF"]').first()
  const [dl] = await Promise.all([p.waitForEvent('download'), sel.selectOption({ index: 1 })])
  const f = await dl.path(); const buf = require('fs').readFileSync(f)
  assert(buf.slice(0, 8).toString() === '%PDF-1.4', 'PDF généré et téléchargé')
  assert(!buf.toString('latin1').includes('{{'), 'champs fusionnés')
  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
