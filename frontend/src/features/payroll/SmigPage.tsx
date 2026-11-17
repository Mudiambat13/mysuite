import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date, money } from '../../lib/format'
import type { SmigReport, SmigStatus, WageGrid, WageGridRow } from '../../lib/types'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  PageHeader,
  Skeleton,
  Table,
  Td,
} from '../../components/ui'

const STATUS: Record<SmigStatus, { label: string; tone: 'green' | 'red' | 'amber' | 'slate' }> = {
  ok: { label: 'Conforme', tone: 'green' },
  below: { label: 'Sous le minimum', tone: 'red' },
  no_class: { label: 'Classe manquante', tone: 'amber' },
  no_salary: { label: 'Salaire manquant', tone: 'amber' },
  no_grid: { label: 'Aucune grille en vigueur', tone: 'amber' },
  currency_mismatch: { label: 'Devises différentes', tone: 'amber' },
}

interface ImportForm {
  name: string
  currency: string
  effective_from: string
  source: string
}

function ImportPanel() {
  const qc = useQueryClient()
  const { register, handleSubmit } = useForm<ImportForm>({ defaultValues: { currency: 'CDF' } })
  const [csv, setCsv] = useState('')
  const [fileName, setFileName] = useState('')
  const [preview, setPreview] = useState<WageGridRow[] | null>(null)
  const run = useMutation({
    mutationFn: (v: ImportForm & { dry_run: boolean }) =>
      api.post<{ rows: number; preview?: WageGridRow[] }>('/wage-grids', { ...v, csv }),
    onSuccess: async (r, v) => {
      if (v.dry_run) setPreview(r.preview ?? [])
      else {
        setPreview(null)
        setCsv('')
        setFileName('')
        await qc.invalidateQueries({ queryKey: ['wage-grids'] })
        await qc.invalidateQueries({ queryKey: ['smig'] })
      }
    },
  })
  return (
    <Card title="Importer une grille (CSV)">
      <p className="mb-3 text-sm text-slate-500">
        Colonnes obligatoires : Classe, Catégorie, Journalier, Mensuel. Le montant mensuel est lu tel quel, il
        n'est jamais recalculé. Vous devez indiquer la source officielle du document.
      </p>
      <form className="grid items-end gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {run.isError && (
          <div className="sm:col-span-2 lg:col-span-4">
            <ErrorBox error={run.error} />
          </div>
        )}
        <Field label="Nom de la grille">
          {(i) => <Input id={i} required {...register('name')} />}
        </Field>
        <Field label="Devise">
          {(i) => <Input id={i} maxLength={3} required {...register('currency')} />}
        </Field>
        <Field label="En vigueur au">
          {(i) => <Input id={i} type="date" required {...register('effective_from')} />}
        </Field>
        <Field label="Source officielle">
          {(i) => <Input id={i} required {...register('source')} />}
        </Field>
        <Field label="Fichier CSV">
          {(i) => (
            <input
              id={i}
              type="file"
              accept=".csv,text/csv"
              className="text-sm"
              onChange={async (e) => {
                const f = e.target.files?.[0]
                setPreview(null)
                setFileName(f?.name ?? '')
                setCsv(f ? await f.text() : '')
              }}
            />
          )}
        </Field>
        <div className="flex gap-2 sm:col-span-2">
          <Button
            type="button"
            variant="secondary"
            disabled={!csv}
            loading={run.isPending}
            onClick={handleSubmit((v) => run.mutate({ ...v, dry_run: true }))}
          >
            Vérifier sans importer
          </Button>
          <Button
            type="button"
            disabled={!csv || !preview}
            loading={run.isPending}
            onClick={handleSubmit((v) => run.mutate({ ...v, dry_run: false }))}
          >
            Importer
          </Button>
        </div>
      </form>
      {fileName && !preview && <p className="mt-2 text-sm text-slate-500">{fileName} : à vérifier avant l'import.</p>}
      {preview && (
        <div className="mt-4">
          <p className="mb-2 text-sm font-medium">{preview.length} lignes valides, contrôlez-les puis importez.</p>
          <Table head={['Classe', 'Catégorie', 'Repère', 'Journalier', 'Mensuel']}>
            {preview.map((r) => (
              <tr key={r.class_no}>
                <Td>{r.class_no}</Td>
                <Td>{r.category}</Td>
                <Td>{r.reference}</Td>
                <Td className="tabular-nums">{r.daily}</Td>
                <Td className="tabular-nums">{r.monthly}</Td>
              </tr>
            ))}
          </Table>
        </div>
      )}
    </Card>
  )
}

export default function SmigPage() {
  const { me, can } = useAuth()
  const key = me?.active_tenant_id
  const grids = useQuery({ queryKey: ['wage-grids', key], queryFn: () => api.get<WageGrid[]>('/wage-grids') })
  const report = useQuery({ queryKey: ['smig', key], queryFn: () => api.get<SmigReport>('/reports/smig-compliance') })
  return (
    <>
      <PageHeader
        title="Grille SMIG et conformité"
        subtitle="Salaires minimums par classe et contrôle des salaires de base"
        actions={
          <a
            href="/api/v1/reports/smig-compliance.csv"
            className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
          >
            <Download className="h-4 w-4" aria-hidden /> Exporter CSV
          </a>
        }
      />
      <div className="space-y-6">
        <p className="rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200">
          Le contrôle est informatif : un salaire sous le minimum ne bloque jamais la paie, il est signalé. La
          grille est celle que vous importez ; MySuite ne garantit pas son exactitude légale.
        </p>
        {can('payroll_rules_manage') && <ImportPanel />}
        <Card title="Grilles importées">
          {grids.isLoading ? (
            <Skeleton rows={2} />
          ) : grids.isError ? (
            <ErrorBox error={grids.error} />
          ) : grids.data!.length === 0 ? (
            <EmptyState title="Aucune grille" hint="Importez un fichier CSV pour activer le contrôle." />
          ) : (
            <Table head={['Nom', 'Devise', 'En vigueur', 'Classes', 'Source']}>
              {grids.data!.map((g) => (
                <tr key={g.id}>
                  <Td className="font-medium">{g.name}</Td>
                  <Td>{g.currency}</Td>
                  <Td>{date(g.effective_from)}</Td>
                  <Td>{g.row_count}</Td>
                  <Td>{g.source}</Td>
                </tr>
              ))}
            </Table>
          )}
        </Card>
        <Card
          title={
            report.data
              ? `Conformité au ${date(report.data.as_of)} : ${report.data.below_minimum} sous le minimum`
              : 'Conformité'
          }
        >
          {report.isLoading ? (
            <Skeleton rows={3} />
          ) : report.isError ? (
            <ErrorBox error={report.error} />
          ) : report.data!.rows.length === 0 ? (
            <EmptyState title="Aucun employé actif" />
          ) : (
            <Table head={['Matricule', 'Employé', 'Classe', 'Salaire de base', 'Minimum', 'Écart', 'Statut']}>
              {report.data!.rows.map((r) => (
                <tr key={r.employee_id}>
                  <Td className="font-mono text-xs">{r.matricule}</Td>
                  <Td>
                    <Link className="text-brand-600 hover:underline" to={`/employees/${r.employee_id}`}>
                      {r.full_name}
                    </Link>
                  </Td>
                  <Td>{r.class_no ?? '—'}</Td>
                  <Td className="tabular-nums">
                    {r.base_salary ? money(r.base_salary, r.salary_currency ?? '') : '—'}
                  </Td>
                  <Td className="tabular-nums">{r.minimum_monthly ?? '—'}</Td>
                  <Td className="tabular-nums">{r.gap ?? '—'}</Td>
                  <Td>
                    <Badge tone={STATUS[r.status].tone}>{STATUS[r.status].label}</Badge>
                  </Td>
                </tr>
              ))}
            </Table>
          )}
        </Card>
      </div>
    </>
  )
}
