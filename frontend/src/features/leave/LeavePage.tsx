import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date } from '../../lib/format'
import type { LeaveBalance, LeaveRequest, LeaveType } from '../../lib/types'
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

function RequestsTable({ scope, canDecide }: { scope: 'mine' | 'team' | 'all'; canDecide: boolean }) {
  const { me } = useAuth()
  const qc = useQueryClient()
  const key = ['leave', scope, me?.active_tenant_id]
  const list = useQuery({
    queryKey: key,
    queryFn: () => api.get<LeaveRequest[]>(`/leave/requests${qs({ scope })}`),
  })
  const decide = useMutation({
    mutationFn: (v: { id: string; approve: boolean }) =>
      api.post(`/leave/requests/${v.id}/decision`, { approve: v.approve }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['leave'] })
      await qc.invalidateQueries({ queryKey: ['dashboard'] })
    },
  })
  const cancel = useMutation({
    mutationFn: (id: string) => api.post(`/leave/requests/${id}/cancel`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['leave'] })
    },
  })
  if (list.isLoading) return <Skeleton rows={3} />
  if (list.isError) return <ErrorBox error={list.error} />
  if (list.data!.length === 0) return <EmptyState title="Aucune demande" />
  return (
    <>
      {(decide.isError || cancel.isError) && (
        <div className="mb-3">
          <ErrorBox error={decide.error ?? cancel.error} />
        </div>
      )}
      <Table head={['Employé', 'Type', 'Période', 'Jours', 'Statut', '']}>
        {list.data!.map((r) => (
          <tr key={r.id}>
            <Td className="font-medium">{r.employee_name}</Td>
            <Td>{r.leave_type_name}</Td>
            <Td>
              {date(r.start_date)} → {date(r.end_date)}
            </Td>
            <Td className="tabular-nums">{r.days}</Td>
            <Td>
              <StatusBadge status={r.status} />
            </Td>
            <Td className="text-right">
              {canDecide && r.status === 'pending' && r.employee_id !== me?.employee_id && (
                <span className="inline-flex gap-2">
                  <Button
                    variant="secondary"
                    onClick={() => decide.mutate({ id: r.id, approve: true })}
                    disabled={decide.isPending}
                  >
                    Approuver
                  </Button>
                  <Button
                    variant="danger"
                    onClick={() => decide.mutate({ id: r.id, approve: false })}
                    disabled={decide.isPending}
                  >
                    Refuser
                  </Button>
                </span>
              )}
              {scope === 'mine' && r.status === 'pending' && (
                <Button variant="ghost" onClick={() => cancel.mutate(r.id)}>
                  Annuler
                </Button>
              )}
            </Td>
          </tr>
        ))}
      </Table>
    </>
  )
}

function NewRequestForm({ types }: { types: LeaveType[] }) {
  const qc = useQueryClient()
  const { register, handleSubmit, reset } = useForm<{
    leave_type_id: string
    start_date: string
    end_date: string
    reason: string
  }>()
  const create = useMutation({
    mutationFn: (v: { leave_type_id: string; start_date: string; end_date: string; reason: string }) =>
      api.post('/leave/requests', { ...v, reason: v.reason || null }),
    onSuccess: async () => {
      reset()
      await qc.invalidateQueries({ queryKey: ['leave'] })
      await qc.invalidateQueries({ queryKey: ['balances'] })
    },
  })
  return (
    <form onSubmit={handleSubmit((v) => create.mutate(v))} className="grid items-end gap-3 sm:grid-cols-5">
      {create.isError && (
        <div className="sm:col-span-5">
          <ErrorBox error={create.error} />
        </div>
      )}
      <Field label="Type">
        {(id) => (
          <Select id={id} required {...register('leave_type_id')}>
            <option value="">Choisir…</option>
            {types
              .filter((t) => t.active)
              .map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
          </Select>
        )}
      </Field>
      <Field label="Du">{(id) => <Input id={id} type="date" required {...register('start_date')} />}</Field>
      <Field label="Au">{(id) => <Input id={id} type="date" required {...register('end_date')} />}</Field>
      <Field label="Motif (facultatif)">{(id) => <Input id={id} {...register('reason')} />}</Field>
      <Button type="submit" loading={create.isPending}>
        Demander
      </Button>
    </form>
  )
}

export default function LeavePage() {
  const { me, can } = useAuth()
  const types = useQuery({
    queryKey: ['leave-types', me?.active_tenant_id],
    queryFn: () => api.get<LeaveType[]>('/leave/types'),
  })
  const balances = useQuery({
    queryKey: ['balances', me?.active_tenant_id],
    queryFn: () => api.get<LeaveBalance[]>('/leave/balances/me'),
    enabled: !!me?.employee_id,
  })
  const canApprove = can('leave_approve')
  const canAll = can('leave_read_all')
  const [tab, setTab] = useState<'mine' | 'team' | 'all'>(canApprove ? 'team' : 'mine')
  const tabs: ['mine' | 'team' | 'all', string, boolean][] = [
    ['mine', 'Mes demandes', !!me?.employee_id],
    ['team', 'À traiter / équipe', canApprove],
    ['all', 'Toute la société', canAll],
  ]

  return (
    <>
      <PageHeader title="Congés" subtitle="Demandes, soldes et approbations" />
      {me?.employee_id && (
        <div className="mb-6 grid gap-4 sm:grid-cols-3">
          {balances.isLoading ? (
            <Skeleton rows={1} />
          ) : (
            balances.data?.map((b) => (
              <Card key={b.leave_type_id}>
                <p className="text-sm text-slate-500">
                  {b.leave_type_name} {b.year}
                </p>
                <p className="mt-1 text-2xl font-semibold tabular-nums">
                  {(Number(b.entitled) - Number(b.used) - Number(b.pending)).toString()}{' '}
                  <span className="text-sm font-normal text-slate-500">jours disponibles</span>
                </p>
                <p className="mt-1 text-xs text-slate-500">
                  Droit {b.entitled} · pris {b.used} · en attente {b.pending}
                </p>
              </Card>
            ))
          )}
        </div>
      )}
      {me?.employee_id && can('leave_request_own') && types.data && (
        <Card title="Nouvelle demande" className="mb-6">
          <NewRequestForm types={types.data} />
        </Card>
      )}
      <Card>
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
                    : 'border-transparent text-slate-500',
                )}
              >
                {label}
              </button>
            ))}
        </div>
        <RequestsTable key={tab} scope={tab} canDecide={canApprove && tab !== 'mine'} />
      </Card>
    </>
  )
}
