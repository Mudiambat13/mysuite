import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { money, periodLabel } from '../../lib/format'
import type { PayslipDetail, RunDetail } from '../../lib/types'
import {
  Badge,
  Button,
  Card,
  ErrorBox,
  Kpi,
  PageHeader,
  Skeleton,
  StatusBadge,
  Table,
  Td,
} from '../../components/ui'
import PayslipView from './PayslipView'

export default function RunDetailPage() {
  const { id = '' } = useParams()
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const [open, setOpen] = useState<string | null>(null)
  const q = useQuery({ queryKey: ['run', id], queryFn: () => api.get<RunDetail>(`/payroll/runs/${id}`) })
  const slip = useQuery({
    queryKey: ['payslip', open],
    queryFn: () => api.get<PayslipDetail>(`/payslips/${open}`),
    enabled: !!open,
  })
  const act = useMutation({
    mutationFn: (a: 'recalculate' | 'validate' | 'close') => api.post(`/payroll/runs/${id}/${a}`),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['run', id] })
      await qc.invalidateQueries({ queryKey: ['runs'] })
    },
  })
  const currency = me?.tenants.find((t) => t.id === me.active_tenant_id)?.currency ?? 'USD'
  if (q.isLoading) return <Skeleton rows={4} />
  if (q.isError) return <ErrorBox error={q.error} />
  const { run, items } = q.data!
  const manage = can('payroll_manage')

  return (
    <>
      <PageHeader
        title={`Paie ${periodLabel(run.year, run.month)}`}
        subtitle={`${run.rule_set_name} v${run.rule_version}`}
        actions={
          <>
            <StatusBadge status={run.status} />
            {run.simulation && <Badge tone="amber">Simulation</Badge>}
          </>
        }
      />
      {run.simulation && (
        <div
          role="note"
          className="mb-4 rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200"
        >
          Simulation : le jeu de règles n'est pas approuvé. Cette paie ne peut pas être validée ni clôturée,
          et les bulletins ne sont pas diffusés aux employés.
        </div>
      )}
      {act.isError && (
        <div className="mb-4">
          <ErrorBox error={act.error} />
        </div>
      )}
      <div className="mb-6 grid gap-4 sm:grid-cols-3">
        <Kpi label="Total brut" value={money(run.total_gross, currency)} />
        <Kpi label="Total net" value={money(run.total_net, currency)} />
        <Kpi label="Coût employeur" value={money(run.total_employer_cost, currency)} />
      </div>
      {manage && run.status !== 'closed' && (
        <div className="mb-6 flex flex-wrap gap-2">
          {run.status === 'draft' && (
            <Button variant="secondary" loading={act.isPending} onClick={() => act.mutate('recalculate')}>
              Recalculer
            </Button>
          )}
          {run.status === 'draft' && (
            <Button disabled={run.simulation} loading={act.isPending} onClick={() => act.mutate('validate')}>
              Valider
            </Button>
          )}
          {run.status === 'validated' && (
            <Button
              loading={act.isPending}
              onClick={() => {
                if (confirm('Clôturer cette paie ? Elle sera définitivement figée.')) act.mutate('close')
              }}
            >
              Clôturer
            </Button>
          )}
        </div>
      )}
      {q.data!.warnings.length > 0 && (
        <ul className="mb-4 list-disc pl-5 text-sm text-amber-800">
          {q.data!.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
      <Card title="Détail par employé">
        <Table head={['Matricule', 'Employé', 'Brut', 'Net', 'Coût employeur', '']}>
          {items.map((i) => (
            <tr key={i.id}>
              <Td className="font-mono text-xs">{i.matricule}</Td>
              <Td className="font-medium">{i.employee_name}</Td>
              <Td className="tabular-nums">{money(i.gross, currency)}</Td>
              <Td className="tabular-nums">{money(i.net, currency)}</Td>
              <Td className="tabular-nums">{money(i.employer_cost, currency)}</Td>
              <Td className="text-right">
                <Button variant="ghost" onClick={() => setOpen(open === i.id ? null : i.id)}>
                  {open === i.id ? 'Masquer' : 'Voir le calcul'}
                </Button>
              </Td>
            </tr>
          ))}
        </Table>
      </Card>
      {open && (
        <div className="mt-6">
          {slip.isLoading ? (
            <Skeleton rows={4} />
          ) : slip.isError ? (
            <ErrorBox error={slip.error} />
          ) : (
            <PayslipView slip={slip.data!} />
          )}
        </div>
      )}
    </>
  )
}
