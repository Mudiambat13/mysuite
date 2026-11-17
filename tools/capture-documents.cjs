// Parcours pièces d'employé : dépôt, liste avec échéance, téléchargement, rejet d'un fichier non accepté, alerte d'expiration.
const { chromium } = require('playwright')
const path = require('path'), fs = require('fs')
const B = 'http://127.0.0.1:5173', OUT = path.join(__dirname, '..', 'docs', 'screenshots'), PWD = 'Demo-Passw0rd-2026'
const assert = (c, m) => { if (!c) throw new Error('ASSERT ' + m); console.log('ok:', m) }
;(async () => {
  const b = await chromium.launch(), errors = []
  const c = await b.newContext({ viewport: { width: 1280, height: 1000 }, deviceScaleFactor: 2, acceptDownloads: true })
  const p = await c.newPage(); p.on('pageerror', (e) => errors.push(e.message))
  await p.goto(B + '/login'); await p.getByLabel('Adresse email').fill('rh@demo.test'); await p.getByLabel('Mot de passe').fill(PWD)
  await p.click('role=button[name="Se connecter"]'); await p.waitForURL(B + '/')
  await p.goto(B + '/employees'); await p.getByRole('link', { name: /Employée/ }).first().click()
  await p.getByRole('tab', { name: 'Documents' }).click(); await p.waitForSelector('text=Pièces du dossier')
  const exp = new Date(Date.now() + 20 * 86400000).toISOString().slice(0, 10)
  const send = async (file, name, expires) => {
    await p.getByLabel('Nom de la pièce').fill(name); await p.getByLabel('Expire le (facultatif)').fill(expires)
    await p.locator('input[type=file]').setInputFiles(file); await p.getByRole('button', { name: 'Déposer' }).click()
  }
  fs.writeFileSync('/tmp/faux.pdf', '<html><script>alert(1)</script>')
  await send('/tmp/faux.pdf', 'Fichier piégé', exp)
  await p.waitForSelector('text=format non accepté'); assert(true, 'contenu non PDF/PNG/JPEG refusé')
  await send('/tmp/cni.pdf', 'Carte d\'identité', exp)
  await p.waitForSelector('td:text-is("Carte d\'identité")'); assert(true, 'pièce déposée')
  assert((await p.locator('text=/dans \\d+ j/').count()) >= 1, 'échéance affichée')
  await p.screenshot({ path: OUT + '/43-documents-employe.png', fullPage: true })
  const [dl] = await Promise.all([p.waitForEvent('download'), p.getByRole('link', { name: 'Télécharger' }).first().click()])
  assert(fs.readFileSync(await dl.path()).toString().startsWith('%PDF-1.4'), 'téléchargement identique')
  await p.goto(B + '/employees'); await p.waitForSelector('text=Pièces expirées ou à renouveler')
  assert(true, 'alerte d\'échéance sur la liste des employés')
  await p.screenshot({ path: OUT + '/44-pieces-a-renouveler.png', fullPage: true })
  // nettoyage : suppression via l'interface
  await p.getByRole('link', { name: /Employée/ }).first().click(); await p.getByRole('tab', { name: 'Documents' }).click()
  p.once('dialog', (d) => d.accept()); await p.getByRole('button', { name: 'Supprimer' }).first().click()
  await p.waitForSelector('text=Aucune pièce'); assert(true, 'suppression')
  assert(errors.length === 0, 'aucune erreur JS : ' + errors.join('|'))
  await b.close()
})().catch((e) => { console.error(e); process.exit(1) })
