import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { Download, Plus, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date } from '../../lib/format'
import type { Department, Employee, ExpiringDocument, Page, Position, Site } from '../../lib/types'
import { KIND_LABEL, expiryBadge } from './DocumentsTab'
import {
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  Modal,
  PageHeader,
  Pagination,
  Select,
  Skeleton,
  StatusBadge,
  Table,
  Td,
} from '../../components/ui'

function ExpiringDocuments() {
  const { me, can } = useAuth()
  const q = useQuery({
    queryKey: ['documents-expiring', me?.active_tenant_id],
    queryFn: () => api.get<ExpiringDocument[]>('/documents/expiring?days=60'),
    enabled: can('documents_read'),
  })
  if (!q.data || q.data.length === 0) return null
  return (
    <Card title={`Pièces expirées ou à renouveler (${q.data.length})`}>
      <ul className="space-y-1 text-sm">
        {q.data.map((d) => (
          <li key={d.id}>
            <Link className="font-medium text-brand-600 hover:underline" to={`/employees/${d.employee_id}`}>
              {d.employee_name}
            </Link>{' '}
            : {d.name} ({KIND_LABEL[d.kind]}), expire le {date(d.expires_on)} {expiryBadge(d.expires_on)}
          </li>
        ))}
      </ul>
    </Card>
  )
}

const STATUSES = ['active', 'on_leave', 'suspended', 'candidate', 'exited']
const CONTRACTS = [
  ['cdi', 'CDI'],
  ['cdd', 'CDD'],
  ['stage', 'Stage'],
  ['journalier', 'Journalier'],
  ['consultant', 'Consultant'],
]

const schema = z.object({
  matricule: z.string().trim().min(1, 'Matricule requis'),
  first_name: z.string().trim().min(1, 'Prénom requis'),
  last_name: z.string().trim().min(1, 'Nom requis'),
  email: z.string().trim().email('Email invalide').or(z.literal('')),
  hire_date: z.string().min(1, "Date d'entrée requise"),
  contract_type: z.string(),
  site_id: z.string(),
  department_id: z.string(),
  position_id: z.string(),
  manager_id: z.string(),
})
type Values = z.infer<typeof schema>
const nul = (s: string) => (s === '' ? null : s)

export function useOrg() {
  const { me } = useAuth()
  const tid = me?.active_tenant_id
  const sites = useQuery({ queryKey: ['sites', tid], queryFn: () => api.get<Site[]>('/org/sites') })
  const departments = useQuery({
    queryKey: ['departments', tid],
    queryFn: () => api.get<Department[]>('/org/departments'),
  })
  const positions = useQuery({
    queryKey: ['positions', tid],
    queryFn: () => api.get<Position[]>('/org/positions'),
  })
  return { sites: sites.data ?? [], departments: departments.data ?? [], positions: positions.data ?? [] }
}

function NewEmployeeModal({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient()
  const org = useOrg()
  const { me } = useAuth()
  const managers = useQuery({
    queryKey: ['managers', me?.active_tenant_id],
    queryFn: () => api.get<Page<Employee>>('/employees?limit=100'),
  })
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: {
      contract_type: 'cdi',
      email: '',
      site_id: '',
      department_id: '',
      position_id: '',
      manager_id: '',
    },
  })
  const create = useMutation({
    mutationFn: (v: Values) =>
      api.post('/employees', {
        ...v,
        email: nul(v.email),
        site_id: nul(v.site_id),
        department_id: nul(v.department_id),
        position_id: nul(v.position_id),
        manager_id: nul(v.manager_id),
      }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['employees'] })
      onClose()
    },
  })
  return (
    <Modal title="Nouvel employé" onClose={onClose}>
      <form onSubmit={handleSubmit((v) => create.mutate(v))} className="grid gap-4 sm:grid-cols-2" noValidate>
        {create.isError && (
          <div className="sm:col-span-2">
            <ErrorBox error={create.error} />
          </div>
        )}
        <Field label="Matricule" error={errors.matricule?.message}>
          {(id) => <Input id={id} {...register('matricule')} />}
        </Field>
        <Field label="Date d'entrée" error={errors.hire_date?.message}>
          {(id) => <Input id={id} type="date" {...register('hire_date')} />}
        </Field>
        <Field label="Prénom" error={errors.first_name?.message}>
          {(id) => <Input id={id} {...register('first_name')} />}
        </Field>
        <Field label="Nom" error={errors.last_name?.message}>
          {(id) => <Input id={id} {...register('last_name')} />}
        </Field>
        <Field label="Email" error={errors.email?.message}>
          {(id) => <Input id={id} type="email" {...register('email')} />}
        </Field>
        <Field label="Contrat">
          {(id) => (
            <Select id={id} {...register('contract_type')}>
              {CONTRACTS.map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Site">
          {(id) => (
            <Select id={id} {...register('site_id')}>
              <option value="">—</option>
              {org.sites.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Département">
          {(id) => (
            <Select id={id} {...register('department_id')}>
              <option value="">—</option>
              {org.departments.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Poste">
          {(id) => (
            <Select id={id} {...register('position_id')}>
              <option value="">—</option>
              {org.positions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Manager">
          {(id) => (
            <Select id={id} {...register('manager_id')}>
              <option value="">—</option>
              {managers.data?.items.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.first_name} {m.last_name}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <div className="flex justify-end gap-2 sm:col-span-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Annuler
          </Button>
          <Button type="submit" loading={create.isPending}>
            Créer
          </Button>
        </div>
      </form>
    </Modal>
  )
}

export default function EmployeesPage() {
  const { can } = useAuth()
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState('')
  const [page, setPage] = useState(1)
  const [creating, setCreating] = useState(false)
  const { me } = useAuth()
  const q = useQuery({
    queryKey: ['employees', me?.active_tenant_id, search, status, page],
    queryFn: () => api.get<Page<Employee>>(`/employees${qs({ search, status, page })}`),
    placeholderData: keepPreviousData,
  })

  return (
    <>
      <PageHeader
        title="Employés"
        subtitle="Dossiers du personnel de la société active"
        actions={
          <>
            <a
              href={`/api/v1/employees/export.csv${qs({ search, status })}`}
              className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              <Download className="h-4 w-4" aria-hidden /> Exporter CSV
            </a>
            {can('employees_manage') && (
              <Button onClick={() => setCreating(true)}>
                <Plus className="h-4 w-4" aria-hidden /> Nouvel employé
              </Button>
            )}
          </>
        }
      />
      <div className="mb-6">
        <ExpiringDocuments />
      </div>
      <Card>
        <div className="mb-4 flex flex-wrap gap-3">
          <div className="relative min-w-60 flex-1">
            <Search
              className="pointer-events-none absolute left-3 top-2.5 h-4 w-4 text-slate-400"
              aria-hidden
            />
            <Input
              aria-label="Rechercher un employé"
              placeholder="Nom, matricule, email…"
              className="pl-9"
              value={search}
              onChange={(e) => {
                setSearch(e.target.value)
                setPage(1)
              }}
            />
          </div>
          <Select
            aria-label="Filtrer par statut"
            className="w-48"
            value={status}
            onChange={(e) => {
              setStatus(e.target.value)
              setPage(1)
            }}
          >
            <option value="">Tous les statuts</option>
            {STATUSES.map((s) => (
              <option key={s} value={s}>
                {t(`status.${s}`)}
              </option>
            ))}
          </Select>
        </div>
        {q.isLoading ? (
          <Skeleton />
        ) : q.isError ? (
          <ErrorBox error={q.error} />
        ) : q.data!.items.length === 0 ? (
          <EmptyState
            title="Aucun employé"
            hint={
              search || status
                ? 'Aucun résultat pour ces filtres.'
                : 'Créez le premier dossier employé pour commencer.'
            }
          />
        ) : (
          <>
            <Table head={['Matricule', 'Nom', 'Département', 'Poste', 'Entrée', 'Statut']}>
              {q.data!.items.map((e) => (
                <tr key={e.id} className="hover:bg-slate-50 dark:hover:bg-slate-800/50">
                  <Td className="font-mono text-xs">{e.matricule}</Td>
                  <Td>
                    <Link to={`/employees/${e.id}`} className="font-medium text-brand-600 hover:underline">
                      {e.first_name} {e.last_name}
                    </Link>
                  </Td>
                  <Td>{e.department_name ?? '—'}</Td>
                  <Td>{e.position_title ?? '—'}</Td>
                  <Td>{date(e.hire_date)}</Td>
                  <Td>
                    <StatusBadge status={e.status} />
                  </Td>
                </tr>
              ))}
            </Table>
            <Pagination page={q.data!.page} limit={q.data!.limit} total={q.data!.total} onChange={setPage} />
          </>
        )}
      </Card>
      {creating && <NewEmployeeModal onClose={() => setCreating(false)} />}
    </>
  )
}
