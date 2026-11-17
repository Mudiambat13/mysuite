import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle } from 'lucide-react'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date } from '../../lib/format'
import type { Amendment, Contract, ContractTemplate, Employee, Page } from '../../lib/types'
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

const TYPES = [
  ['cdi', 'CDI'],
  ['cdd', 'CDD'],
  ['stage', 'Stage'],
  ['journalier', 'Journalier'],
  ['consultant', 'Consultant'],
]

function daysUntil(d: string): number {
  return Math.ceil((new Date(`${d}T00:00:00`).getTime() - Date.now()) / 86_400_000)
}

function Amendments({ contract, canManage }: { contract: Contract; canManage: boolean }) {
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['amendments', contract.id],
    queryFn: () => api.get<Amendment[]>(`/contracts/${contract.id}/amendments`),
  })
  const { register, handleSubmit, reset } = useForm<{
    effective_date: string
    description: string
    new_end_date: string
  }>()
  const add = useMutation({
    mutationFn: (v: { effective_date: string; description: string; new_end_date: string }) =>
      api.post(`/contracts/${contract.id}/amendments`, { ...v, new_end_date: v.new_end_date || null }),
    onSuccess: async () => {
      reset()
      await qc.invalidateQueries({ queryKey: ['amendments', contract.id] })
      await qc.invalidateQueries({ queryKey: ['contracts'] })
    },
  })
  return (
    <div className="mt-3 rounded-lg bg-slate-50 p-4 dark:bg-slate-800/50">
      <h3 className="mb-2 text-sm font-semibold">Avenants — {contract.employee_name}</h3>
      {list.isLoading ? (
        <Skeleton rows={1} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <p className="text-sm text-slate-500">Aucun avenant.</p>
      ) : (
        <ul className="space-y-1 text-sm">
          {list.data!.map((a) => (
            <li key={a.id}>
              {date(a.effective_date)} — {a.description}
              {a.new_end_date ? ` (nouvelle fin : ${date(a.new_end_date)})` : ''}
            </li>
          ))}
        </ul>
      )}
      {canManage && contract.status === 'active' && (
        <form
          onSubmit={handleSubmit((v) => add.mutate(v))}
          className="mt-3 grid items-end gap-3 sm:grid-cols-4"
        >
          {add.isError && (
            <div className="sm:col-span-4">
              <ErrorBox error={add.error} />
            </div>
          )}
          <Field label="Date d'effet">
            {(id) => <Input id={id} type="date" required {...register('effective_date')} />}
          </Field>
          <Field label="Description">{(id) => <Input id={id} required {...register('description')} />}</Field>
          <Field label="Nouvelle fin (facultatif)">
            {(id) => <Input id={id} type="date" {...register('new_end_date')} />}
          </Field>
          <Button type="submit" loading={add.isPending}>
            Ajouter l'avenant
          </Button>
        </form>
      )}
    </div>
  )
}

const MERGE_FIELDS = [
  'company',
  'employee_name',
  'matricule',
  'job_title',
  'contract_type',
  'start_date',
  'end_date',
  'probation_end',
]

function Templates({ canManage }: { canManage: boolean }) {
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['contract-templates'],
    queryFn: () => api.get<ContractTemplate[]>('/contract-templates'),
  })
  const { register, handleSubmit, reset } = useForm<{ name: string; body: string }>()
  const create = useMutation({
    mutationFn: (v: { name: string; body: string }) => api.post('/contract-templates', v),
    onSuccess: async () => {
      reset()
      await qc.invalidateQueries({ queryKey: ['contract-templates'] })
    },
  })
  return (
    <Card title="Modèles de contrat (texte fourni par la société)">
      <p className="mb-3 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
        MySuite ne fournit aucune clause juridique : saisissez votre texte, à faire valider par un juriste. Seuls
        les champs de fusion suivants sont remplacés :{' '}
        {MERGE_FIELDS.map((f) => (
          <code key={f} className="mr-1">{`{{${f}}}`}</code>
        ))}
        . Le salaire n'est jamais fusionné.
      </p>
      {list.isLoading ? (
        <Skeleton rows={1} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <EmptyState title="Aucun modèle" hint="Créez un modèle pour générer des contrats en PDF." />
      ) : (
        <Table head={['Nom', 'Créé le']}>
          {list.data!.map((t) => (
            <tr key={t.id}>
              <Td className="font-medium">{t.name}</Td>
              <Td>{date(t.created_at)}</Td>
            </tr>
          ))}
        </Table>
      )}
      {canManage && (
        <form onSubmit={handleSubmit((v) => create.mutate(v))} className="mt-4 grid gap-3">
          {create.isError && <ErrorBox error={create.error} />}
          <Field label="Nom du modèle">{(i) => <Input id={i} required {...register('name')} />}</Field>
          <Field label="Texte du contrat">
            {(i) => (
              <textarea
                id={i}
                required
                rows={8}
                className="w-full rounded-lg border border-slate-300 bg-white p-3 text-sm dark:border-slate-700 dark:bg-slate-900"
                {...register('body')}
              />
            )}
          </Field>
          <div>
            <Button type="submit" loading={create.isPending}>
              Enregistrer le modèle
            </Button>
          </div>
        </form>
      )}
    </Card>
  )
}

export default function ContractsPage() {
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const tid = me?.active_tenant_id
  const canManage = can('contracts_manage')
  const templates = useQuery({
    queryKey: ['contract-templates'],
    queryFn: () => api.get<ContractTemplate[]>('/contract-templates'),
  })
  const [openId, setOpenId] = useState<string | null>(null)
  const contracts = useQuery({
    queryKey: ['contracts', tid],
    queryFn: () => api.get<Contract[]>(`/contracts${qs({})}`),
  })
  const expiring = useQuery({
    queryKey: ['contracts-expiring', tid],
    queryFn: () => api.get<Contract[]>('/contracts/expiring?days=60'),
  })
  const employees = useQuery({
    queryKey: ['employees-select', tid],
    queryFn: () => api.get<Page<Employee>>('/employees?limit=100'),
    enabled: canManage,
  })
  const { register, handleSubmit, reset } = useForm<{
    employee_id: string
    contract_type: string
    start_date: string
    end_date: string
    probation_end: string
    job_title: string
  }>({ defaultValues: { contract_type: 'cdi' } })
  const create = useMutation({
    mutationFn: (v: {
      employee_id: string
      contract_type: string
      start_date: string
      end_date: string
      probation_end: string
      job_title: string
    }) =>
      api.post('/contracts', {
        ...v,
        end_date: v.end_date || null,
        probation_end: v.probation_end || null,
        job_title: v.job_title || null,
      }),
    onSuccess: async () => {
      reset({ contract_type: 'cdi' })
      await qc.invalidateQueries({ queryKey: ['contracts'] })
      await qc.invalidateQueries({ queryKey: ['contracts-expiring'] })
    },
  })
  const terminate = useMutation({
    mutationFn: (id: string) =>
      api.post(`/contracts/${id}/terminate`, { end_date: new Date().toISOString().slice(0, 10) }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['contracts'] })
      await qc.invalidateQueries({ queryKey: ['contracts-expiring'] })
    },
  })

  return (
    <>
      <PageHeader title="Contrats" subtitle="Contrats, périodes d'essai, avenants et échéances" />
      <div className="space-y-6">
        {expiring.data && expiring.data.length > 0 && (
          <Card
            title={
              <span className="flex items-center gap-2">
                <AlertTriangle className="h-4 w-4 text-amber-600" aria-hidden /> Échéances dans les 60 jours
              </span>
            }
          >
            <ul className="space-y-1 text-sm">
              {expiring.data.map((c) => {
                const target = c.end_date ?? c.probation_end!
                const n = daysUntil(target)
                return (
                  <li key={c.id}>
                    <span className="font-medium">{c.employee_name}</span> —{' '}
                    {c.end_date ? 'fin de contrat' : "fin de période d'essai"} le {date(target)}{' '}
                    <Badge tone={n < 0 ? 'red' : n <= 15 ? 'amber' : 'slate'}>
                      {n < 0 ? 'dépassée' : `dans ${n} j`}
                    </Badge>
                  </li>
                )
              })}
            </ul>
          </Card>
        )}
        <Card title="Tous les contrats">
          {terminate.isError && (
            <div className="mb-3">
              <ErrorBox error={terminate.error} />
            </div>
          )}
          {contracts.isLoading ? (
            <Skeleton />
          ) : contracts.isError ? (
            <ErrorBox error={contracts.error} />
          ) : contracts.data!.length === 0 ? (
            <EmptyState title="Aucun contrat" hint="Créez un contrat pour un employé ci-dessous." />
          ) : (
            <Table head={['Employé', 'Type', 'Début', 'Fin', "Essai jusqu'au", 'Statut', '']}>
              {contracts.data!.map((c) => (
                <tr key={c.id}>
                  <Td className="font-medium">
                    {c.employee_name} <span className="font-mono text-xs text-slate-500">{c.matricule}</span>
                  </Td>
                  <Td>{c.contract_type.toUpperCase()}</Td>
                  <Td>{date(c.start_date)}</Td>
                  <Td>{date(c.end_date)}</Td>
                  <Td>{date(c.probation_end)}</Td>
                  <Td>
                    <StatusBadge status={c.status} />
                  </Td>
                  <Td className="text-right">
                    <span className="inline-flex gap-2">
                      <Button variant="ghost" onClick={() => setOpenId(openId === c.id ? null : c.id)}>
                        Avenants
                      </Button>
                      {(templates.data?.length ?? 0) > 0 && (
                        <Select
                          aria-label={`Générer le PDF de ${c.employee_name}`}
                          value=""
                          onChange={(e) => {
                            if (e.target.value)
                              window.location.href = `/api/v1/contracts/${c.id}/document.pdf?template_id=${e.target.value}`
                          }}
                        >
                          <option value="">Générer le PDF…</option>
                          {templates.data!.map((t) => (
                            <option key={t.id} value={t.id}>
                              {t.name}
                            </option>
                          ))}
                        </Select>
                      )}
                      {canManage && c.status === 'active' && (
                        <Button
                          variant="secondary"
                          onClick={() => {
                            if (confirm(`Résilier le contrat de ${c.employee_name} à la date du jour ?`))
                              terminate.mutate(c.id)
                          }}
                        >
                          Résilier
                        </Button>
                      )}
                    </span>
                  </Td>
                </tr>
              ))}
            </Table>
          )}
          {openId && contracts.data?.find((c) => c.id === openId) && (
            <Amendments contract={contracts.data.find((c) => c.id === openId)!} canManage={canManage} />
          )}
        </Card>
        <Templates canManage={canManage} />
        {canManage && (
          <Card title="Nouveau contrat">
            <form
              onSubmit={handleSubmit((v) => create.mutate(v))}
              className="grid items-end gap-3 sm:grid-cols-3"
            >
              {create.isError && (
                <div className="sm:col-span-3">
                  <ErrorBox error={create.error} />
                </div>
              )}
              <Field label="Employé">
                {(id) => (
                  <Select id={id} required {...register('employee_id')}>
                    <option value="">Choisir…</option>
                    {employees.data?.items.map((e) => (
                      <option key={e.id} value={e.id}>
                        {e.first_name} {e.last_name} ({e.matricule})
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label="Type">
                {(id) => (
                  <Select id={id} {...register('contract_type')}>
                    {TYPES.map(([v, l]) => (
                      <option key={v} value={v}>
                        {l}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
              <Field label="Intitulé du poste">{(id) => <Input id={id} {...register('job_title')} />}</Field>
              <Field label="Début">
                {(id) => <Input id={id} type="date" required {...register('start_date')} />}
              </Field>
              <Field label="Fin (obligatoire pour un CDD)">
                {(id) => <Input id={id} type="date" {...register('end_date')} />}
              </Field>
              <Field label="Fin de période d'essai">
                {(id) => <Input id={id} type="date" {...register('probation_end')} />}
              </Field>
              <Button type="submit" loading={create.isPending}>
                Créer le contrat
              </Button>
            </form>
          </Card>
        )}
      </div>
    </>
  )
}
