import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle } from 'lucide-react'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { money, periodLabel, date } from '../../lib/format'
import type { Period, Run, RuleSet, RuleTemplate } from '../../lib/types'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  PageHeader,
  Select,
  Skeleton,
  StatusBadge,
  Table,
  Td,
} from '../../components/ui'

const SIMULATION_NOTICE =
  "Mode simulation : aucun jeu de règles approuvé n'est utilisé. Ces résultats ne sont pas utilisables comme paie réelle."

function TemplatePanel() {
  const qc = useQueryClient()
  const tpl = useQuery({
    queryKey: ['rule-templates'],
    queryFn: () => api.get<RuleTemplate[]>('/payroll/rule-set-templates'),
  })
  const { me } = useAuth()
  const tenantCurrency = me?.tenants.find((x) => x.id === me.active_tenant_id)?.currency
  const [open, setOpen] = useState(false)
  const create = useMutation({
    mutationFn: (t: RuleTemplate) =>
      api.post('/payroll/rule-sets', {
        country: t.country,
        name: t.name,
        effective_from: t.effective_from,
        params: t.params,
        source: `Modèle non vérifié (sources secondaires, consulté le ${t.consulte_le})`,
      }),
    onSuccess: async () => {
      setOpen(false)
      await qc.invalidateQueries({ queryKey: ['rule-sets'] })
    },
  })
  if (!tpl.data || tpl.data.length === 0) return null
  return (
    <div className="mb-4 rounded-lg border border-slate-200 p-4 dark:border-slate-700">
      {tpl.data.map((t) => (
        <div key={t.id}>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <p className="font-medium">
                {t.name} — {t.currency}, {t.period}
              </p>
              <p className="text-sm text-slate-500">
                Brouillon à faire valider : aucune valeur n'est vérifiée.
              </p>
              {t.variante && <p className="mt-1 text-sm text-amber-700 dark:text-amber-300">{t.variante}</p>}
            </div>
            <div className="flex gap-2">
              <Button variant="secondary" onClick={() => setOpen(!open)}>
                {open ? 'Masquer le détail' : 'Voir le détail et les sources'}
              </Button>
              <Button loading={create.isPending} onClick={() => create.mutate(t)}>
                Créer un brouillon depuis ce modèle
              </Button>
            </div>
          </div>
          {tenantCurrency && tenantCurrency !== t.currency && (
            <p
              role="alert"
              className="mt-3 rounded bg-red-50 p-3 text-sm text-red-900 dark:bg-red-950 dark:text-red-200"
            >
              Ce modèle est exprimé en {t.currency} (barème mensuel) mais la devise de la société est{' '}
              {tenantCurrency} : les montants seraient comparés à un barème dans une autre devise. MySuite ne
              convertit pas les devises ; ne l'utilisez pas en l'état.
            </p>
          )}
          {create.isError && (
            <div className="mt-3">
              <ErrorBox error={create.error} />
            </div>
          )}
          {open && (
            <div className="mt-4 space-y-4 text-sm">
              <p className="rounded bg-amber-50 p-3 text-amber-900 dark:bg-amber-950 dark:text-amber-200">
                {t.avertissement}
              </p>
              <Table head={['Paramètre', 'Valeur', 'Niveau de confiance']}>
                {t.provenance.map((p) => (
                  <tr key={p.champ}>
                    <Td className="font-mono text-xs">{p.champ}</Td>
                    <Td>{p.valeur}</Td>
                    <Td>{p.confiance}</Td>
                  </tr>
                ))}
              </Table>
              <div>
                <h4 className="mb-1 font-semibold">À compléter par un expert</h4>
                <ul className="list-disc space-y-1 pl-5">
                  {t.a_completer.map((a) => (
                    <li key={a}>{a}</li>
                  ))}
                </ul>
              </div>
              <div>
                <h4 className="mb-1 font-semibold">Sources (consultées le {t.consulte_le})</h4>
                <ul className="list-disc space-y-1 pl-5">
                  {Object.entries(t.sources).map(([k, s]) => (
                    <li key={k}>
                      <a
                        href={s.url}
                        target="_blank"
                        rel="noreferrer noopener"
                        className="text-brand-600 underline"
                      >
                        {s.titre}
                      </a>{' '}
                      <span className="text-slate-500">({s.type})</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}
        </div>
      ))}
    </div>
  )
}

function RuleSets() {
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['rule-sets', me?.active_tenant_id],
    queryFn: () => api.get<RuleSet[]>('/payroll/rule-sets'),
  })
  const [approving, setApproving] = useState<string | null>(null)
  const { register, handleSubmit } = useForm<{ source: string; verified_on: string }>()
  const approve = useMutation({
    mutationFn: (v: { id: string; source: string; verified_on: string }) =>
      api.post(`/payroll/rule-sets/${v.id}/approve`, { source: v.source, verified_on: v.verified_on }),
    onSuccess: async () => {
      setApproving(null)
      await qc.invalidateQueries({ queryKey: ['rule-sets'] })
    },
  })
  return (
    <Card title="Jeux de règles de paie (versionnés)">
      <p className="mb-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
        Les taux et barèmes légaux ne sont jamais fournis par MySuite. Un jeu de règles doit être saisi, puis
        vérifié par un expert local et approuvé par un second utilisateur avant toute paie réelle.
      </p>
      {can('payroll_rules_manage') && <TemplatePanel />}
      {list.isLoading ? (
        <Skeleton rows={2} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <EmptyState
          title="Aucun jeu de règles"
          hint="Partez du modèle ci-dessus, ou créez-en un via l'API (POST /api/v1/payroll/rule-sets)."
        />
      ) : (
        <Table head={['Nom', 'Pays', 'Version', 'En vigueur', 'Statut', '']}>
          {list.data!.map((r) => (
            <tr key={r.id}>
              <Td className="font-medium">
                {r.name} {r.is_demo && <Badge tone="amber">Démo fictif</Badge>}
              </Td>
              <Td>{r.country}</Td>
              <Td>v{r.version}</Td>
              <Td>{date(r.effective_from)}</Td>
              <Td>
                <StatusBadge status={r.status} />
              </Td>
              <Td className="text-right">
                {can('payroll_rules_manage') && r.status === 'draft' && !r.is_demo && (
                  <Button variant="secondary" onClick={() => setApproving(r.id)}>
                    Approuver…
                  </Button>
                )}
              </Td>
            </tr>
          ))}
        </Table>
      )}
      {approving && (
        <form
          onSubmit={handleSubmit((v) => approve.mutate({ id: approving, ...v }))}
          className="mt-4 grid items-end gap-3 rounded-lg border border-slate-200 p-4 sm:grid-cols-3 dark:border-slate-700"
        >
          {approve.isError && (
            <div className="sm:col-span-3">
              <ErrorBox error={approve.error} />
            </div>
          )}
          <Field label="Source officielle (texte, article)">
            {(id) => <Input id={id} required {...register('source')} />}
          </Field>
          <Field label="Date de vérification">
            {(id) => <Input id={id} type="date" required {...register('verified_on')} />}
          </Field>
          <div className="flex gap-2">
            <Button type="submit" loading={approve.isPending}>
              Confirmer l'approbation
            </Button>
            <Button type="button" variant="secondary" onClick={() => setApproving(null)}>
              Annuler
            </Button>
          </div>
        </form>
      )}
    </Card>
  )
}

export default function PayrollPage() {
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const key = me?.active_tenant_id
  const runs = useQuery({ queryKey: ['runs', key], queryFn: () => api.get<Run[]>('/payroll/runs') })
  const periods = useQuery({
    queryKey: ['periods', key],
    queryFn: () => api.get<Period[]>('/payroll/periods'),
  })
  const ruleSets = useQuery({
    queryKey: ['rule-sets', key],
    queryFn: () => api.get<RuleSet[]>('/payroll/rule-sets'),
  })
  const pf = useForm<{ year: string; month: string }>({
    defaultValues: { year: String(new Date().getFullYear()), month: String(new Date().getMonth() + 1) },
  })
  const rf = useForm<{ period_id: string; rule_set_id: string }>()
  const addPeriod = useMutation({
    mutationFn: (v: { year: string; month: string }) =>
      api.post('/payroll/periods', { year: Number(v.year), month: Number(v.month) }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['periods'] })
    },
  })
  const addRun = useMutation({
    mutationFn: (v: { period_id: string; rule_set_id: string }) => api.post('/payroll/runs', v),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['runs'] })
    },
  })
  const manage = can('payroll_manage')
  const openPeriods = (periods.data ?? []).filter((p) => p.status === 'open')
  const usable = (ruleSets.data ?? []).filter((r) => r.status !== 'retired')

  return (
    <>
      <PageHeader
        title="Paie"
        subtitle="Périodes, calculs, validation et clôture"
        actions={
          can('compensation_read') ? (
            <Link
              to="/payroll/smig"
              className="rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              Grille SMIG et conformité
            </Link>
          ) : undefined
        }
      />
      <div className="space-y-6">
        {manage && (
          <Card title="Préparer une paie">
            <div className="grid gap-6 lg:grid-cols-2">
              <form
                onSubmit={pf.handleSubmit((v) => addPeriod.mutate(v))}
                className="grid items-end gap-3 sm:grid-cols-3"
              >
                {addPeriod.isError && (
                  <div className="sm:col-span-3">
                    <ErrorBox error={addPeriod.error} />
                  </div>
                )}
                <Field label="Année">
                  {(id) => <Input id={id} type="number" required {...pf.register('year')} />}
                </Field>
                <Field label="Mois">
                  {(id) => (
                    <Input id={id} type="number" min={1} max={12} required {...pf.register('month')} />
                  )}
                </Field>
                <Button type="submit" variant="secondary" loading={addPeriod.isPending}>
                  Créer la période
                </Button>
              </form>
              <form
                onSubmit={rf.handleSubmit((v) => addRun.mutate(v))}
                className="grid items-end gap-3 sm:grid-cols-3"
              >
                {addRun.isError && (
                  <div className="sm:col-span-3">
                    <ErrorBox error={addRun.error} />
                  </div>
                )}
                <Field label="Période">
                  {(id) => (
                    <Select id={id} required {...rf.register('period_id')}>
                      <option value="">Choisir…</option>
                      {openPeriods.map((p) => (
                        <option key={p.id} value={p.id}>
                          {periodLabel(p.year, p.month)}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Field label="Règles">
                  {(id) => (
                    <Select id={id} required {...rf.register('rule_set_id')}>
                      <option value="">Choisir…</option>
                      {usable.map((r) => (
                        <option key={r.id} value={r.id}>
                          {r.name} v{r.version} ({r.status})
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Button type="submit" loading={addRun.isPending}>
                  Calculer
                </Button>
              </form>
            </div>
          </Card>
        )}
        <Card title="Exécutions de paie">
          {runs.isLoading ? (
            <Skeleton />
          ) : runs.isError ? (
            <ErrorBox error={runs.error} />
          ) : runs.data!.length === 0 ? (
            <EmptyState title="Aucune paie" hint="Créez une période puis lancez un calcul." />
          ) : (
            <Table head={['Période', 'Règles', 'Employés', 'Brut', 'Net', 'Statut', '']}>
              {runs.data!.map((r) => (
                <tr key={r.id}>
                  <Td className="font-medium">{periodLabel(r.year, r.month)}</Td>
                  <Td>
                    {r.rule_set_name} v{r.rule_version}
                  </Td>
                  <Td>{r.employee_count}</Td>
                  <Td className="tabular-nums">{money(r.total_gross)}</Td>
                  <Td className="tabular-nums">{money(r.total_net)}</Td>
                  <Td>
                    <span className="inline-flex items-center gap-2">
                      <StatusBadge status={r.status} />
                      {r.simulation && <Badge tone="amber">Simulation</Badge>}
                    </span>
                  </Td>
                  <Td className="text-right">
                    <Link className="text-brand-600 hover:underline" to={`/payroll/runs/${r.id}`}>
                      Détail
                    </Link>
                  </Td>
                </tr>
              ))}
            </Table>
          )}
          <p className="mt-3 flex items-start gap-2 text-xs text-slate-500">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
            {SIMULATION_NOTICE}
          </p>
        </Card>
        <RuleSets />
      </div>
    </>
  )
}
