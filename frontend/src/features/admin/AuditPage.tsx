import { useState } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { dateTime } from '../../lib/format'
import type { AuditRow, Page } from '../../lib/types'
import { Card, EmptyState, ErrorBox, PageHeader, Pagination, Skeleton, Table, Td } from '../../components/ui'

export default function AuditPage() {
  const { me } = useAuth()
  const [page, setPage] = useState(1)
  const q = useQuery({
    queryKey: ['audit', me?.active_tenant_id, page],
    queryFn: () => api.get<Page<AuditRow>>(`/audit${qs({ page, limit: 50 })}`),
    placeholderData: keepPreviousData,
  })
  return (
    <>
      <PageHeader title="Journal d'audit" subtitle="Actions sensibles, en ajout seul (non modifiable)" />
      <Card>
        {q.isLoading ? (
          <Skeleton />
        ) : q.isError ? (
          <ErrorBox error={q.error} />
        ) : q.data!.items.length === 0 ? (
          <EmptyState title="Aucune entrée" />
        ) : (
          <>
            <Table head={['Date', 'Auteur', 'Action', 'Objet']}>
              {q.data!.items.map((a) => (
                <tr key={a.id}>
                  <Td className="whitespace-nowrap">{dateTime(a.created_at)}</Td>
                  <Td>{a.actor_name ?? '—'}</Td>
                  <Td className="font-mono text-xs">{a.action}</Td>
                  <Td className="text-xs text-slate-500">
                    {a.entity}
                    {a.entity_id ? ` · ${a.entity_id.slice(0, 8)}` : ''}
                  </Td>
                </tr>
              ))}
            </Table>
            <Pagination page={q.data!.page} limit={q.data!.limit} total={q.data!.total} onChange={setPage} />
          </>
        )}
      </Card>
    </>
  )
}
