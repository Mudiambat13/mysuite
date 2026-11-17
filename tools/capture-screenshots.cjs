// Régénère docs/screenshots (nécessite l'API sur :8080 avec `seed-demo` sur une base vierge, Vite sur :5173 et Playwright).
// Usage : NODE_PATH=$(npm root -g) node tools/capture-screenshots.cjs
const { chromium } = require('playwright')
const B = 'http://127.0.0.1:5173'
const OUT = require('path').join(__dirname, '..', 'docs', 'screenshots')
const PWD = 'Demo-Passw0rd-2026'
const H = { 'X-Requested-With': 'mysuite', 'Content-Type': 'application/json' }
const iso = (n) => new Date(Date.now() + n * 86400000).toISOString().slice(0, 10)

;(async () => {
  const b = await chromium.launch()
  const newCtx = async (opts = {}) => { const c = await b.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, ...opts }); return c }
  const login = async (c, email) => {
    const r = await c.request.post(B + '/api/v1/auth/login', { headers: H, data: { email, password: PWD } })
    if (!r.ok()) throw new Error('login ' + email + ' ' + r.status())
  }
  const api = async (c, method, path, data) => {
    const r = await c.request.fetch(B + '/api/v1' + path, { method, headers: H, data })
    if (!r.ok()) throw new Error(method + ' ' + path + ' ' + r.status() + ' ' + (await r.text()))
    const t = await r.text(); return t ? JSON.parse(t) : null
  }

  // ---- Données de démonstration supplémentaires (fictives) ----
  const hr = await newCtx(); await login(hr, 'rh@demo.test')
  const emps = (await api(hr, 'GET', '/employees?limit=100')).items
  const by = (m) => emps.find((e) => e.matricule === m)
  await api(hr, 'POST', '/contracts', { employee_id: by('M-001').id, contract_type: 'cdi', start_date: '2022-01-10', job_title: 'Chef d\'équipe', probation_end: null })
  await api(hr, 'POST', '/contracts', { employee_id: by('M-005').id, contract_type: 'cdd', start_date: '2026-01-05', end_date: iso(24), job_title: 'Agent' })
  await api(hr, 'POST', '/contracts', { employee_id: by('M-004').id, contract_type: 'cdi', start_date: iso(-80), probation_end: iso(12), job_title: 'Agent' })
  await api(hr, 'POST', '/contracts', { employee_id: by('M-003').id, contract_type: 'cdi', start_date: '2023-03-01', job_title: 'Agent' })
  const c5 = (await api(hr, 'GET', '/contracts?employee_id=' + by('M-005').id))[0]
  await api(hr, 'POST', `/contracts/${c5.id}/amendments`, { effective_date: iso(-10), description: 'Révision du lieu d\'affectation' })

  const eva = await newCtx(); await login(eva, 'employe@demo.test')
  const types = await api(eva, 'GET', '/leave/types')
  const annual = types.find((t) => t.code === 'ANNUAL').id
  await api(hr, 'PUT', '/leave/balances', { employee_id: by('M-002').id, leave_type_id: annual, year: 2027, entitled: '20' })
  await api(eva, 'POST', '/leave/requests', { leave_type_id: annual, start_date: '2026-12-21', end_date: '2026-12-23', reason: 'Fêtes de fin d\'année' })
  await api(eva, 'POST', '/leave/requests', { leave_type_id: annual, start_date: '2027-01-11', end_date: '2027-01-13', reason: 'Démarches personnelles' })
  const mgr = await newCtx(); await login(mgr, 'manager@demo.test')
  const team = await api(mgr, 'GET', '/leave/requests?scope=team&status=pending')
  await api(mgr, 'POST', `/leave/requests/${team[0].id}/decision`, { approve: true, comment: 'Validé' })

  const pay = await newCtx(); await login(pay, 'paie1@demo.test')
  const period = await api(pay, 'POST', '/payroll/periods', { year: 2026, month: 11 })
  const rs = (await api(pay, 'GET', '/payroll/rule-sets'))[0]
  await api(pay, 'POST', '/payroll/runs', { period_id: period.id, rule_set_id: rs.id })
  await api(pay, 'POST', `/employees/${by('M-002').id}/pay-items`, { kind: 'allowance', code: 'TRANSPORT', label: 'Indemnité de transport', amount: '50.00', active_from: '2026-01-01' })
  const run = (await api(pay, 'GET', '/payroll/runs'))[0]
  await api(pay, 'POST', `/payroll/runs/${run.id}/recalculate`)
  const detail = await api(pay, 'GET', `/payroll/runs/${run.id}`)
  const evaItem = detail.items.find((i) => i.matricule === 'M-002').id

  // ---- Captures ----
  const shot = async (c, path, name, { wait, full = false, action } = {}) => {
    const p = await c.newPage()
    await p.goto(B + path)
    if (wait) await p.waitForSelector(wait)
    if (action) await action(p)
    await p.waitForTimeout(900)
    await p.screenshot({ path: `${OUT}/${name}.png`, fullPage: full })
    await p.close(); console.log('ok', name)
  }

  // 01 connexion (sans session)
  const anon = await newCtx(); await shot(anon, '/login', '01-connexion', { wait: 'text=Se connecter' })
  // 02 tableau de bord RH
  const hrB = await newCtx(); await login(hrB, 'rh@demo.test')
  await shot(hrB, '/', '02-tableau-de-bord', { wait: 'text=Effectif actif' })
  // 03 employés
  await shot(hrB, '/employees', '03-employes', { wait: 'text=Kasongo' })
  // 04 fiche employé — rémunération (accès restreint, rôle paie)
  const payB = await newCtx(); await login(payB, 'paie1@demo.test')
  await shot(payB, '/employees/' + by('M-002').id, '04-fiche-employe-remuneration', { wait: 'text=Informations', full: true, action: async (p) => { await p.click('role=tab[name="Rémunération"]'); await p.waitForSelector('text=Historique salarial') } })
  // 05 contrats
  await shot(hrB, '/contracts', '05-contrats', { wait: 'text=Nouveau contrat', full: true, action: async (p) => { await p.waitForSelector('text=Échéances'); await p.click('button:has-text("Avenants") >> nth=0'); await p.waitForSelector('text=Avenants —') } })
  // 06 congés (vue manager : demandes de l'équipe)
  const mgrB = await newCtx(); await login(mgrB, 'manager@demo.test')
  await shot(mgrB, '/leave', '06-conges-manager', { wait: 'text=Approuvé', full: true })
  // 07 congés côté employé (soldes + demande)
  const evaB = await newCtx(); await login(evaB, 'employe@demo.test')
  await shot(evaB, '/leave', '07-conges-employe', { wait: 'text=jours disponibles', full: true })
  // 08 paie : liste + règles
  await shot(payB, '/payroll', '08-paie', { wait: 'text=Exécutions de paie', full: true })
  // 09 paie : détail + calcul explicable
  await shot(payB, '/payroll/runs/' + run.id, '09-paie-detail-calcul', { wait: 'text=Détail par employé', full: true, action: async (p) => { await p.click('tr:has-text("M-002") >> text=Voir le calcul'); await p.waitForSelector('text=Net à payer') } })
  // 10 journal d'audit
  await shot(hrB, '/audit', '10-audit', { wait: 'text=contract.create' })
  // 11 mode sombre
  const dark = await newCtx({ colorScheme: 'dark' }); await login(dark, 'rh@demo.test')
  await shot(dark, '/', '11-mode-sombre', { wait: 'text=Effectif actif' })
  // 12 refus d'accès : l'employé n'a pas la paie (garde d'affichage + 403 serveur)
  const r = await evaB.request.get(B + '/api/v1/payroll/runs'); console.log('employee payroll API status', r.status())
  const slipOther = await evaB.request.get(B + '/api/v1/payslips/' + evaItem); console.log('employee own simulated payslip status', slipOther.status())
  await b.close()
})().catch((e) => { console.log('FAIL', e.message.split('\n').slice(0, 5).join(' | ')); process.exit(1) })
