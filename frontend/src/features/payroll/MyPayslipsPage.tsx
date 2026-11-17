import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { money, periodLabel } from '../../lib/format'
import type { PayslipDetail, PayslipSummary } from '../../lib/types'
import { Button, Card, EmptyState, ErrorBox, PageHeader, Skeleton, Table, Td } from '../../components/ui'
import PayslipView from './PayslipView'

export default function MyPayslipsPage() {
  const { me } = useAuth()
  const [open, setOpen] = useState<string | null>(null)
  const list = useQuery({
    queryKey: ['my-payslips', me?.active_tenant_id],
    queryFn: () => api.get<PayslipSummary[]>('/payslips/me'),
  })
  const slip = useQuery({
    queryKey: ['payslip', open],
    queryFn: () => api.get<PayslipDetail>(`/payslips/${open}`),
    enabled: !!open,
  })
  const currency = me?.tenants.find((t) => t.id === me.active_tenant_id)?.currency ?? 'USD'
  return (
    <>
      <PageHeader title="Mes bulletins" subtitle="Bulletins des paies clôturées" />
      <Card>
        {list.isLoading ? (
          <Skeleton rows={3} />
        ) : list.isError ? (
          <ErrorBox error={list.error} />
        ) : list.data!.length === 0 ? (
          <EmptyState
            title="Aucun bulletin disponible"
            hint="Vos bulletins apparaîtront ici une fois la paie clôturée."
          />
        ) : (
          <Table head={['Période', 'Brut', 'Net', '']}>
            {list.data!.map((p) => (
              <tr key={p.item_id}>
                <Td className="font-medium">{periodLabel(p.year, p.month)}</Td>
                <Td className="tabular-nums">{money(p.gross, currency)}</Td>
                <Td className="tabular-nums">{money(p.net, currency)}</Td>
                <Td className="text-right">
                  <Button variant="ghost" onClick={() => setOpen(open === p.item_id ? null : p.item_id)}>
                    {open === p.item_id ? 'Masquer' : 'Consulter'}
                  </Button>
                </Td>
              </tr>
            ))}
          </Table>
        )}
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
