import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { InteractiveChart } from '../../components/charts'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date, money, monthName } from '../../lib/format'
import type { Absences, Movements, PayrollCost } from '../../lib/types'
import {
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  Kpi,
  PageHeader,
  Select,
  Skeleton,
  Table,
  Td,
  cx,
} from '../../components/ui'
import { useOrg } from '../employees/EmployeesPage'

type Tab = 'movements' | 'absences' | 'payroll'

const COLORS = { entries: '#3b6cf6', exits: '#e0803a', gross: '#3b6cf6', net: '#2f9e75', cost: '#8a6bd6' }

function DownloadLink({ href }: { href: string }) {
  return (
    <a
      href={href}
      className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
    >
      <Download className="h-4 w-4" aria-hidden /> Exporter CSV
    </a>
  )
}

function MovementsTab({ params }: { params: Record<string, string> }) {
  const { me } = useAuth()
  const q = useQuery({
    queryKey: ['rep-movements', me?.active_tenant_id, params],
    queryFn: () => api.get<Movements>(`/reports/movements${qs(params)}`),
  })
  if (q.isLoading) return <Skeleton rows={4} />
  if (q.isError) return <ErrorBox error={q.error} />
  const m = q.data!
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="grid flex-1 gap-4 sm:grid-cols-3">
          <Kpi label="Entrées" value={m.entries.length} />
          <Kpi label="Sorties" value={m.exits.length} />
          <Kpi
            label="Solde net"
            value={`${m.entries.length - m.exits.length >= 0 ? '+' : ''}${m.entries.length - m.exits.length}`}
          />
        </div>
        <DownloadLink href={`/api/v1/reports/movements.csv${qs(params)}`} />
      </div>
      <Card title="Entrées et sorties par mois">
        <InteractiveChart
          label="Entrées et sorties par mois"
          data={m.by_month}
          xKey="month"
          series={[
            { key: 'entries', name: 'Entrées', color: COLORS.entries },
            { key: 'exits', name: 'Sorties', color: COLORS.exits },
          ]}
        />
      </Card>
      <div className="grid gap-6 lg:grid-cols-2">
        {(
          [
            ['Entrées', m.entries],
            ['Sorties', m.exits],
          ] as const
        ).map(([title, rows]) => (
          <Card key={title} title={title}>
            {rows.length === 0 ? (
              <EmptyState title="Aucun mouvement sur la période" />
            ) : (
              <Table head={['Matricule', 'Nom', 'Département', 'Date']}>
                {rows.map((r) => (
                  <tr key={r.employee_id}>
                    <Td className="font-mono text-xs">{r.matricule}</Td>
                    <Td className="font-medium">{r.name}</Td>
                    <Td>{r.department_name ?? '—'}</Td>
                    <Td>{date(r.date)}</Td>
                  </tr>
                ))}
              </Table>
            )}
          </Card>
        ))}
      </div>
    </div>
  )
}

function AbsencesTab({ params }: { params: Record<string, string> }) {
  const { me } = useAuth()
  const q = useQuery({
    queryKey: ['rep-absences', me?.active_tenant_id, params],
    queryFn: () => api.get<Absences>(`/reports/absences${qs(params)}`),
  })
  if (q.isLoading) return <Skeleton rows={4} />
  if (q.isError) return <ErrorBox error={q.error} />
  const a = q.data!
  const chart = (rows: Absences['by_type']) => rows.map((r) => ({ label: r.label, jours: Number(r.days) }))
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="grid flex-1 gap-4 sm:grid-cols-2">
          <Kpi
            label="Jours d'absence approuvés"
            value={Number(a.total_days)}
            hint="Congés dont le début tombe dans la période"
          />
          <Kpi label="Demandes approuvées" value={a.by_type.reduce((n, r) => n + r.requests, 0)} />
        </div>
        <DownloadLink href={`/api/v1/reports/absences.csv${qs(params)}`} />
      </div>
      {a.by_type.length === 0 ? (
        <Card>
          <EmptyState
            title="Aucune absence approuvée sur la période"
            hint="Élargissez les dates ou retirez un filtre."
          />
        </Card>
      ) : (
        <div className="grid gap-6 lg:grid-cols-2">
          {(
            [
              ['Par type de congé', a.by_type],
              ['Par département', a.by_department],
            ] as const
          ).map(([title, rows]) => (
            <Card key={title} title={title}>
              <InteractiveChart
                label={title}
                data={chart(rows)}
                xKey="label"
                series={[{ key: 'jours', name: 'Jours', color: COLORS.entries }]}
                kinds={['bar']}
                height={224}
              />
              <Table head={['Libellé', 'Jours', 'Demandes']}>
                {rows.map((r) => (
                  <tr key={r.label}>
                    <Td className="font-medium">{r.label}</Td>
                    <Td className="tabular-nums">{r.days}</Td>
                    <Td>{r.requests}</Td>
                  </tr>
                ))}
              </Table>
            </Card>
          ))}
        </div>
      )}
    </div>
  )
}

function PayrollTab({ year }: { year: string }) {
  const { me } = useAuth()
  const q = useQuery({
    queryKey: ['rep-cost', me?.active_tenant_id, year],
    queryFn: () => api.get<PayrollCost>(`/reports/payroll-cost${qs({ year })}`),
  })
  if (q.isLoading) return <Skeleton rows={4} />
  if (q.isError) return <ErrorBox error={q.error} />
  const c = q.data!
  const sum = (f: (m: PayrollCost['months'][number]) => string) =>
    c.months.reduce((n, m) => n + Number(f(m)), 0)
  const data = c.months.map((m) => ({
    mois: monthName(m.month).slice(0, 4),
    Brut: Number(m.gross),
    Net: Number(m.net),
    'Coût employeur': Number(m.employer_cost),
  }))
  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="max-w-2xl text-sm text-slate-500">
          Seules les paies <strong>clôturées et non simulées</strong> sont comptées. La ventilation suit le
          département actuel de chaque employé.
        </p>
        <DownloadLink href={`/api/v1/reports/payroll-cost.csv${qs({ year })}`} />
      </div>
      {c.months.length === 0 ? (
        <Card>
          <EmptyState
            title={`Aucune paie clôturée en ${c.year}`}
            hint="Les simulations et les paies non clôturées ne sont jamais comptées dans la masse salariale."
          />
        </Card>
      ) : (
        <>
          <div className="grid gap-4 sm:grid-cols-3">
            <Kpi
              label="Brut cumulé"
              value={money(
                sum((m) => m.gross),
                c.currency,
              )}
            />
            <Kpi
              label="Net cumulé"
              value={money(
                sum((m) => m.net),
                c.currency,
              )}
            />
            <Kpi
              label="Coût employeur cumulé"
              value={money(
                sum((m) => m.employer_cost),
                c.currency,
              )}
            />
          </div>
          <Card title={`Masse salariale ${c.year} par mois`}>
            <InteractiveChart
              label="Masse salariale par mois"
              data={data}
              xKey="mois"
              format={(n) => money(String(n), c.currency)}
              series={[
                { key: 'Brut', name: 'Brut', color: COLORS.gross },
                { key: 'Net', name: 'Net', color: COLORS.net },
                { key: 'Coût employeur', name: 'Coût employeur', color: COLORS.cost },
              ]}
            />
          </Card>
          <Card title="Par département">
            <Table head={['Département', 'Employés', 'Brut', 'Coût employeur']}>
              {c.by_department.map((d) => (
                <tr key={d.label}>
                  <Td className="font-medium">{d.label}</Td>
                  <Td>{d.employees}</Td>
                  <Td className="tabular-nums">{money(d.gross, c.currency)}</Td>
                  <Td className="tabular-nums">{money(d.employer_cost, c.currency)}</Td>
                </tr>
              ))}
            </Table>
          </Card>
        </>
      )}
    </div>
  )
}

export default function ReportsPage() {
  const { can } = useAuth()
  const org = useOrg()
  const now = new Date()
  const [tab, setTab] = useState<Tab>('movements')
  const [from, setFrom] = useState(`${now.getFullYear()}-01-01`)
  const [to, setTo] = useState(now.toISOString().slice(0, 10))
  const [department, setDepartment] = useState('')
  const [site, setSite] = useState('')
  const [year, setYear] = useState(String(now.getFullYear()))
  const tabs: [Tab, string, boolean][] = [
    ['movements', 'Mouvements', true],
    ['absences', 'Absences', true],
    ['payroll', 'Masse salariale', can('payroll_read')],
  ]
  const params: Record<string, string> = { from, to, department_id: department, site_id: site }

  return (
    <>
      <PageHeader title="Rapports" subtitle="Indicateurs calculés côté serveur, exportables en CSV" />
      <div
        className="mb-4 flex gap-1 overflow-x-auto border-b border-slate-200 dark:border-slate-800"
        role="tablist"
      >
        {tabs
          .filter((t) => t[2])
          .map(([k, label]) => (
            <button
              key={k}
              role="tab"
              aria-selected={tab === k}
              onClick={() => setTab(k)}
              className={cx(
                '-mb-px whitespace-nowrap border-b-2 px-4 py-2 text-sm font-medium',
                tab === k
                  ? 'border-brand-600 text-brand-700 dark:text-brand-100'
                  : 'border-transparent text-slate-500 hover:text-slate-800',
              )}
            >
              {label}
            </button>
          ))}
      </div>
      <Card className="mb-6">
        {tab === 'payroll' ? (
          <div className="max-w-xs">
            <Field label="Année">
              {(id) => (
                <Input
                  id={id}
                  type="number"
                  min={2000}
                  max={2100}
                  value={year}
                  onChange={(e) => setYear(e.target.value)}
                />
              )}
            </Field>
          </div>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <Field label="Du">
              {(id) => <Input id={id} type="date" value={from} onChange={(e) => setFrom(e.target.value)} />}
            </Field>
            <Field label="Au">
              {(id) => <Input id={id} type="date" value={to} onChange={(e) => setTo(e.target.value)} />}
            </Field>
            <Field label="Département">
              {(id) => (
                <Select id={id} value={department} onChange={(e) => setDepartment(e.target.value)}>
                  <option value="">Tous</option>
                  {org.departments.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label="Site">
              {(id) => (
                <Select id={id} value={site} onChange={(e) => setSite(e.target.value)}>
                  <option value="">Tous</option>
                  {org.sites.map((s) => (
                    <option key={s.id} value={s.id}>
                      {s.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
          </div>
        )}
      </Card>
      {tab === 'movements' && <MovementsTab params={params} />}
      {tab === 'absences' && <AbsencesTab params={params} />}
      {tab === 'payroll' && can('payroll_read') && <PayrollTab year={year} />}
    </>
  )
}
