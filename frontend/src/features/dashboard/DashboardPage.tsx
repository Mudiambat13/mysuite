import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Link } from 'react-router-dom'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import type { Dashboard, Headcount } from '../../lib/types'
import { InteractiveChart, InteractiveDonut, Segmented } from '../../components/charts'
import { Card, EmptyState, ErrorBox, Kpi, PageHeader, Skeleton } from '../../components/ui'

function BucketChart({ data, donut }: { data: { label: string; count: number }[]; donut: boolean }) {
  if (data.length === 0)
    return <EmptyState title="Aucune donnée" hint="Ajoutez des employés pour voir la répartition." />
  if (donut) return <InteractiveDonut data={data} label="Répartition de l'effectif" />
  return (
    <InteractiveChart
      data={data}
      xKey="label"
      series={[{ key: 'count', name: 'Effectif' }]}
      label="Répartition de l'effectif"
      height={224}
      kinds={['bar']}
    />
  )
}

export default function DashboardPage() {
  const { me, can } = useAuth()
  const { t } = useTranslation()
  const [dim, setDim] = useState<'by_department' | 'by_site' | 'by_contract' | 'by_status'>('by_department')
  const [view, setView] = useState<'donut' | 'bar'>('donut')
  const dash = useQuery({
    queryKey: ['dashboard', me?.active_tenant_id],
    queryFn: () => api.get<Dashboard>('/dashboard'),
    enabled: !!me?.active_tenant_id,
  })
  const head = useQuery({
    queryKey: ['headcount', me?.active_tenant_id],
    queryFn: () => api.get<Headcount>('/reports/headcount'),
    enabled: can('reports_read'),
  })

  if (!me?.active_tenant_id) {
    return (
      <EmptyState
        title="Aucune société active"
        hint="Votre compte n'est rattaché à aucune société. Utilisez l'écran « Sociétés » pour en créer une."
      />
    )
  }
  if (dash.isLoading) return <Skeleton rows={3} />
  if (dash.isError) return <ErrorBox error={dash.error} />
  const d = dash.data!

  return (
    <>
      <PageHeader
        title={`Bonjour ${me.user.full_name.split(' ')[0]}`}
        subtitle="Vue d'ensemble de votre activité RH"
      />
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {d.active_employees !== null && <Kpi label="Effectif actif" value={d.active_employees} />}
        {d.on_leave_today !== null && (
          <Kpi label="Absents aujourd'hui" value={d.on_leave_today} hint="Congés approuvés" />
        )}
        {d.pending_leave_to_review !== null && (
          <Link to="/leave" className="block">
            <Kpi label="Demandes à traiter" value={d.pending_leave_to_review} hint="Congés en attente" />
          </Link>
        )}
        <Kpi label="Mes demandes en attente" value={d.my_pending_leave} />
      </div>
      {can('reports_read') && (
        <Card
          className="mt-6"
          title="Répartition de l'effectif"
          actions={
            <div className="flex flex-wrap gap-2">
              <Segmented
                label="Dimension"
                value={dim}
                onChange={setDim}
                options={[
                  ['by_department', 'Département'],
                  ['by_site', 'Site'],
                  ['by_contract', 'Contrat'],
                  ['by_status', 'Statut'],
                ]}
              />
              <Segmented
                label="Vue"
                value={view}
                onChange={setView}
                options={[
                  ['donut', 'Anneau'],
                  ['bar', 'Barres'],
                ]}
              />
            </div>
          }
        >
          {head.isLoading ? (
            <Skeleton rows={3} />
          ) : head.isError ? (
            <ErrorBox error={head.error} />
          ) : (
            <BucketChart
              donut={view === 'donut'}
              data={head.data![dim].map((b) => ({
                ...b,
                label: dim === 'by_status' ? t(`status.${b.label}`, { defaultValue: b.label }) : b.label,
              }))}
            />
          )}
        </Card>
      )}
    </>
  )
}
