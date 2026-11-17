import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date, money } from '../../lib/format'
import DocumentsTab from './DocumentsTab'
import type { Compensation, Dependents, Employee, PayItem, WageClass } from '../../lib/types'
import {
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
  cx,
} from '../../components/ui'

type Tab = 'profile' | 'compensation' | 'documents'

function Info({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div>
      <dt className="text-xs uppercase tracking-wide text-slate-500">{label}</dt>
      <dd className="mt-0.5 text-sm">{value || '—'}</dd>
    </div>
  )
}

function CompensationTab({ id }: { id: string }) {
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const currency = me?.tenants.find((t) => t.id === me.active_tenant_id)?.currency ?? 'USD'
  const comp = useQuery({
    queryKey: ['comp', id],
    queryFn: () => api.get<Compensation[]>(`/employees/${id}/compensation`),
  })
  const items = useQuery({
    queryKey: ['payitems', id],
    queryFn: () => api.get<PayItem[]>(`/employees/${id}/pay-items`),
  })
  const cf = useForm<{ base_salary: string; effective_from: string; currency: string }>({
    defaultValues: { currency },
  })
  const pf = useForm<{ kind: string; code: string; label: string; amount: string; active_from: string }>({
    defaultValues: { kind: 'allowance' },
  })
  const addComp = useMutation({
    mutationFn: (v: { base_salary: string; effective_from: string; currency: string }) =>
      api.post(`/employees/${id}/compensation`, v),
    onSuccess: async () => {
      cf.reset()
      await qc.invalidateQueries({ queryKey: ['comp', id] })
    },
  })
  const addItem = useMutation({
    mutationFn: (v: { kind: string; code: string; label: string; amount: string; active_from: string }) =>
      api.post(`/employees/${id}/pay-items`, v),
    onSuccess: async () => {
      pf.reset({ kind: 'allowance' })
      await qc.invalidateQueries({ queryKey: ['payitems', id] })
    },
  })
  const deps = useQuery({
    queryKey: ['dependents', id],
    queryFn: () => api.get<Dependents[]>(`/employees/${id}/dependents`),
  })
  const df = useForm<{ dependents: string; effective_from: string }>()
  const addDeps = useMutation({
    mutationFn: (v: { dependents: string; effective_from: string }) =>
      api.post(`/employees/${id}/dependents`, {
        dependents: Number(v.dependents),
        effective_from: v.effective_from,
      }),
    onSuccess: async () => {
      df.reset()
      await qc.invalidateQueries({ queryKey: ['dependents', id] })
    },
  })
  const classes = useQuery({
    queryKey: ['wage-class', id],
    queryFn: () => api.get<WageClass[]>(`/employees/${id}/wage-class`),
  })
  const wf = useForm<{ class_no: string; effective_from: string }>()
  const addClass = useMutation({
    mutationFn: (v: { class_no: string; effective_from: string }) =>
      api.post(`/employees/${id}/wage-class`, { class_no: Number(v.class_no), effective_from: v.effective_from }),
    onSuccess: async () => {
      wf.reset()
      await qc.invalidateQueries({ queryKey: ['wage-class', id] })
    },
  })
  const manage = can('compensation_manage')

  return (
    <div className="space-y-6">
      <Card title="Historique salarial (accès restreint et tracé)">
        {comp.isLoading ? (
          <Skeleton rows={2} />
        ) : comp.isError ? (
          <ErrorBox error={comp.error} />
        ) : comp.data!.length === 0 ? (
          <EmptyState
            title="Aucune rémunération"
            hint="Ajoutez un salaire de base pour inclure cet employé dans la paie."
          />
        ) : (
          <Table head={['Effectif au', 'Salaire de base']}>
            {comp.data!.map((c) => (
              <tr key={c.id}>
                <Td>{date(c.effective_from)}</Td>
                <Td className="tabular-nums">{money(c.base_salary, c.currency)}</Td>
              </tr>
            ))}
          </Table>
        )}
        {manage && (
          <form
            onSubmit={cf.handleSubmit((v) => addComp.mutate(v))}
            className="mt-4 grid items-end gap-3 sm:grid-cols-4"
          >
            {addComp.isError && (
              <div className="sm:col-span-4">
                <ErrorBox error={addComp.error} />
              </div>
            )}
            <Field label="Salaire de base">
              {(i) => <Input id={i} inputMode="decimal" required {...cf.register('base_salary')} />}
            </Field>
            <Field label="Devise">
              {(i) => <Input id={i} maxLength={3} required {...cf.register('currency')} />}
            </Field>
            <Field label="Effectif au">
              {(i) => <Input id={i} type="date" required {...cf.register('effective_from')} />}
            </Field>
            <Button type="submit" loading={addComp.isPending}>
              Ajouter
            </Button>
          </form>
        )}
      </Card>
      <Card title="Personnes fiscalement à charge (réduction d'impôt si prévue par les règles)">
        {deps.isLoading ? (
          <Skeleton rows={1} />
        ) : deps.isError ? (
          <ErrorBox error={deps.error} />
        ) : deps.data!.length === 0 ? (
          <EmptyState
            title="Aucune valeur"
            hint="Sans valeur, 0 personne à charge est utilisé dans le calcul."
          />
        ) : (
          <Table head={['Effectif au', 'Personnes à charge']}>
            {deps.data!.map((d) => (
              <tr key={d.id}>
                <Td>{date(d.effective_from)}</Td>
                <Td className="tabular-nums">{d.dependents}</Td>
              </tr>
            ))}
          </Table>
        )}
        {manage && (
          <form
            onSubmit={df.handleSubmit((v) => addDeps.mutate(v))}
            className="mt-4 grid items-end gap-3 sm:grid-cols-3"
          >
            {addDeps.isError && (
              <div className="sm:col-span-3">
                <ErrorBox error={addDeps.error} />
              </div>
            )}
            <Field label="Personnes à charge">
              {(i) => <Input id={i} type="number" min={0} max={30} required {...df.register('dependents')} />}
            </Field>
            <Field label="Effectif au">
              {(i) => <Input id={i} type="date" required {...df.register('effective_from')} />}
            </Field>
            <Button type="submit" loading={addDeps.isPending}>
              Enregistrer
            </Button>
          </form>
        )}
      </Card>
      <Card title="Classe de la grille SMIG (contrôle du salaire minimum)">
        {classes.isLoading ? (
          <Skeleton rows={1} />
        ) : classes.isError ? (
          <ErrorBox error={classes.error} />
        ) : classes.data!.length === 0 ? (
          <EmptyState
            title="Aucune classe"
            hint="Sans classe, aucun contrôle SMIG ni plancher de cotisation n'est appliqué à cet employé."
          />
        ) : (
          <Table head={['Effectif au', 'Classe']}>
            {classes.data!.map((c) => (
              <tr key={c.id}>
                <Td>{date(c.effective_from)}</Td>
                <Td className="tabular-nums">{c.class_no}</Td>
              </tr>
            ))}
          </Table>
        )}
        {manage && (
          <form
            onSubmit={wf.handleSubmit((v) => addClass.mutate(v))}
            className="mt-4 grid items-end gap-3 sm:grid-cols-3"
          >
            {addClass.isError && (
              <div className="sm:col-span-3">
                <ErrorBox error={addClass.error} />
              </div>
            )}
            <Field label="Classe">
              {(i) => <Input id={i} type="number" min={1} required {...wf.register('class_no')} />}
            </Field>
            <Field label="Effectif au">
              {(i) => <Input id={i} type="date" required {...wf.register('effective_from')} />}
            </Field>
            <Button type="submit" loading={addClass.isPending}>
              Enregistrer
            </Button>
          </form>
        )}
      </Card>
      <Card title="Indemnités et retenues récurrentes">
        {items.isLoading ? (
          <Skeleton rows={2} />
        ) : items.isError ? (
          <ErrorBox error={items.error} />
        ) : items.data!.length === 0 ? (
          <EmptyState title="Aucun élément" />
        ) : (
          <Table head={['Type', 'Code', 'Libellé', 'Montant', 'Début', 'Fin']}>
            {items.data!.map((p) => (
              <tr key={p.id}>
                <Td>{p.kind === 'allowance' ? 'Indemnité' : 'Retenue'}</Td>
                <Td className="font-mono text-xs">{p.code}</Td>
                <Td>{p.label}</Td>
                <Td className="tabular-nums">{money(p.amount, currency)}</Td>
                <Td>{date(p.active_from)}</Td>
                <Td>{date(p.active_to)}</Td>
              </tr>
            ))}
          </Table>
        )}
        {manage && (
          <form
            onSubmit={pf.handleSubmit((v) => addItem.mutate(v))}
            className="mt-4 grid items-end gap-3 sm:grid-cols-6"
          >
            {addItem.isError && (
              <div className="sm:col-span-6">
                <ErrorBox error={addItem.error} />
              </div>
            )}
            <Field label="Type">
              {(i) => (
                <Select id={i} {...pf.register('kind')}>
                  <option value="allowance">Indemnité</option>
                  <option value="deduction">Retenue</option>
                </Select>
              )}
            </Field>
            <Field label="Code">{(i) => <Input id={i} required {...pf.register('code')} />}</Field>
            <Field label="Libellé">{(i) => <Input id={i} required {...pf.register('label')} />}</Field>
            <Field label="Montant">
              {(i) => <Input id={i} inputMode="decimal" required {...pf.register('amount')} />}
            </Field>
            <Field label="Début">
              {(i) => <Input id={i} type="date" required {...pf.register('active_from')} />}
            </Field>
            <Button type="submit" loading={addItem.isPending}>
              Ajouter
            </Button>
          </form>
        )}
      </Card>
    </div>
  )
}

export default function EmployeeDetailPage() {
  const { id = '' } = useParams()
  const { can } = useAuth()
  const [tab, setTab] = useState<Tab>('profile')
  const q = useQuery({ queryKey: ['employee', id], queryFn: () => api.get<Employee>(`/employees/${id}`) })
  if (q.isLoading) return <Skeleton rows={5} />
  if (q.isError) return <ErrorBox error={q.error} />
  const e = q.data!
  const tabs: [Tab, string, boolean][] = [
    ['profile', 'Profil', true],
    ['compensation', 'Rémunération', can('compensation_read')],
    ['documents', 'Documents', can('documents_read')],
  ]

  return (
    <>
      <PageHeader
        title={`${e.first_name} ${e.last_name}`}
        subtitle={`${e.matricule} · ${e.position_title ?? 'Poste non défini'}`}
        actions={<StatusBadge status={e.status} />}
      />
      <div className="mb-4 flex gap-1 border-b border-slate-200 dark:border-slate-800" role="tablist">
        {tabs
          .filter((t) => t[2])
          .map(([k, label]) => (
            <button
              key={k}
              role="tab"
              aria-selected={tab === k}
              onClick={() => setTab(k)}
              className={cx(
                '-mb-px border-b-2 px-4 py-2 text-sm font-medium',
                tab === k
                  ? 'border-brand-600 text-brand-700 dark:text-brand-100'
                  : 'border-transparent text-slate-500 hover:text-slate-800',
              )}
            >
              {label}
            </button>
          ))}
      </div>
      {tab === 'profile' ? (
        <Card title="Informations">
          <dl className="grid gap-4 sm:grid-cols-3">
            <Info label="Email" value={e.email} />
            <Info label="Téléphone" value={e.phone} />
            <Info label="Naissance" value={e.birth_date ? date(e.birth_date) : null} />
            <Info label="Entrée" value={date(e.hire_date)} />
            <Info label="Sortie" value={e.exit_date ? date(e.exit_date) : null} />
            <Info label="Contrat" value={e.contract_type.toUpperCase()} />
            <Info label="Département" value={e.department_name} />
            <Info label="Site" value={e.site_name} />
            <Info label="Poste" value={e.position_title} />
          </dl>
        </Card>
      ) : tab === 'documents' ? (
        <DocumentsTab id={id} />
      ) : (
        <CompensationTab id={id} />
      )}
    </>
  )
}
