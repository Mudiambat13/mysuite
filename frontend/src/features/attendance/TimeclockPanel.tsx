import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy, KeyRound, Trash2, Upload } from 'lucide-react'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { dateTime } from '../../lib/format'
import type {
  Badge,
  CreatedDevice,
  Device,
  Employee,
  IngestResult,
  Page,
  PunchOutcome,
  RawPunch,
} from '../../lib/types'
import {
  Badge as Tag,
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  Kpi,
  Modal,
  Select,
  Skeleton,
  Table,
  Td,
} from '../../components/ui'

const STATUS: Record<string, [string, 'green' | 'blue' | 'slate' | 'amber' | 'red']> = {
  in: ['Entrée créée', 'green'],
  out: ['Sortie enregistrée', 'blue'],
  duplicate: ['Doublon ignoré', 'slate'],
  unknown_badge: ['Badge inconnu', 'amber'],
  rejected: ['Refusé', 'red'],
}

function StatusTag({ status }: { status: string }) {
  const [label, tone] = STATUS[status] ?? [status, 'slate' as const]
  return <Tag tone={tone}>{label}</Tag>
}

// ---------- Appareils ----------

function Devices() {
  const { me } = useAuth()
  const qc = useQueryClient()
  const [name, setName] = useState('')
  const [created, setCreated] = useState<CreatedDevice | null>(null)
  const list = useQuery({
    queryKey: ['attendance', 'devices', me?.active_tenant_id],
    queryFn: () => api.get<Device[]>('/attendance/devices'),
  })
  const create = useMutation({
    mutationFn: () => api.post<CreatedDevice>('/attendance/devices', { name }),
    onSuccess: async (d) => {
      setCreated(d)
      setName('')
      await qc.invalidateQueries({ queryKey: ['attendance', 'devices'] })
    },
  })
  const revoke = useMutation({
    mutationFn: (id: string) => api.post(`/attendance/devices/${id}/revoke`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['attendance', 'devices'] }),
  })
  const origin = typeof window === 'undefined' ? '' : window.location.origin
  return (
    <Card title="Badgeuses connectées (clé d’API)">
      <p className="mb-4 max-w-3xl text-sm text-slate-500">
        Chaque badgeuse (ou passerelle) envoie ses passages avec sa propre clé. La clé n’est affichée qu’une
        fois et peut être révoquée à tout moment ; elle ne permet rien d’autre que d’envoyer des passages pour
        votre société.
      </p>
      <form
        className="mb-4 flex flex-wrap items-end gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (name.trim()) create.mutate()
        }}
      >
        <div className="w-72">
          <Field label="Nouvelle badgeuse">
            {(id) => (
              <Input
                id={id}
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Ex. Entrée principale"
                maxLength={80}
              />
            )}
          </Field>
        </div>
        <Button type="submit" loading={create.isPending} disabled={!name.trim()}>
          <KeyRound className="h-4 w-4" aria-hidden /> Générer une clé
        </Button>
      </form>
      {create.isError && <ErrorBox error={create.error} />}
      {list.isLoading ? (
        <Skeleton rows={2} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <EmptyState
          title="Aucune badgeuse déclarée"
          hint="Vous pouvez aussi importer un fichier CSV exporté de votre badgeuse."
        />
      ) : (
        <Table head={['Nom', 'Clé', 'Dernier envoi', 'État', '']}>
          {list.data!.map((d) => (
            <tr key={d.id}>
              <Td className="font-medium">{d.name}</Td>
              <Td className="font-mono text-xs">{d.key_prefix}…</Td>
              <Td>{d.last_seen_at ? dateTime(d.last_seen_at) : 'Jamais'}</Td>
              <Td>{d.active ? <Tag tone="green">Active</Tag> : <Tag tone="slate">Révoquée</Tag>}</Td>
              <Td>
                {d.active && (
                  <Button
                    variant="ghost"
                    onClick={() =>
                      confirm(`Révoquer « ${d.name} » ? La badgeuse ne pourra plus envoyer de passages.`) &&
                      revoke.mutate(d.id)
                    }
                  >
                    Révoquer
                  </Button>
                )}
              </Td>
            </tr>
          ))}
        </Table>
      )}
      {created && (
        <Modal title="Clé de la badgeuse" onClose={() => setCreated(null)}>
          <p className="mb-3 text-sm">
            Copiez cette clé maintenant : <strong>elle ne sera plus jamais affichée</strong>.
          </p>
          <div className="flex items-center gap-2 rounded-lg bg-slate-100 p-3 font-mono text-xs break-all dark:bg-slate-800">
            <span className="flex-1" data-testid="api-key">
              {created.api_key}
            </span>
            <Button
              variant="ghost"
              aria-label="Copier la clé"
              onClick={() => navigator.clipboard?.writeText(created.api_key)}
            >
              <Copy className="h-4 w-4" aria-hidden />
            </Button>
          </div>
          <p className="mt-4 mb-1 text-sm font-medium">Exemple d’envoi</p>
          <pre className="overflow-x-auto rounded-lg bg-slate-900 p-3 text-xs text-slate-100">{`curl -X POST ${origin}/api/v1/device/punches \\
  -H "Authorization: Bearer ${created.api_key}" \\
  -H "Content-Type: application/json" \\
  -d '{"punches":[{"badge":"A1B2C3","at":"2026-03-02T08:03:00+02:00"}]}'`}</pre>
          <p className="mt-2 text-xs text-slate-500">
            « at » doit inclure le décalage horaire. « direction » (« entrée »/« sortie ») est facultative :
            sans elle, un passage ouvre le pointage s’il n’y en a pas, ou le ferme. Les renvois sont sans
            effet (idempotent).
          </p>
          <div className="mt-4 flex justify-end">
            <Button onClick={() => setCreated(null)}>J’ai copié la clé</Button>
          </div>
        </Modal>
      )}
    </Card>
  )
}

// ---------- Badges ----------

function Badges() {
  const { me } = useAuth()
  const qc = useQueryClient()
  const [employee, setEmployee] = useState('')
  const [code, setCode] = useState('')
  const list = useQuery({
    queryKey: ['attendance', 'badges', me?.active_tenant_id],
    queryFn: () => api.get<Badge[]>('/attendance/badges'),
  })
  const emps = useQuery({
    queryKey: ['attendance', 'employees'],
    queryFn: () => api.get<Page<Employee>>('/employees?limit=100&status=active'),
  })
  const add = useMutation({
    mutationFn: () => api.post('/attendance/badges', { employee_id: employee, badge_code: code }),
    onSuccess: async () => {
      setCode('')
      await qc.invalidateQueries({ queryKey: ['attendance', 'badges'] })
    },
  })
  const del = useMutation({
    mutationFn: (id: string) => api.del(`/attendance/badges/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['attendance', 'badges'] }),
  })
  return (
    <Card title="Badges des employés">
      <p className="mb-4 max-w-3xl text-sm text-slate-500">
        Associez le code lu par la badgeuse à un employé. Sans association, le <strong>matricule</strong> sert
        d’identifiant.
      </p>
      <form
        className="mb-4 flex flex-wrap items-end gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (employee && code.trim()) add.mutate()
        }}
      >
        <div className="w-72">
          <Field label="Employé">
            {(id) => (
              <Select id={id} value={employee} onChange={(e) => setEmployee(e.target.value)}>
                <option value="">Choisir…</option>
                {emps.data?.items.map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.matricule} — {e.first_name} {e.last_name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        <div className="w-56">
          <Field label="Code du badge">
            {(id) => <Input id={id} value={code} onChange={(e) => setCode(e.target.value)} maxLength={64} />}
          </Field>
        </div>
        <Button type="submit" loading={add.isPending} disabled={!employee || !code.trim()}>
          Associer
        </Button>
      </form>
      {add.isError && <ErrorBox error={add.error} />}
      {list.isLoading ? (
        <Skeleton rows={2} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <EmptyState title="Aucun badge associé" />
      ) : (
        <Table head={['Employé', 'Matricule', 'Code badge', '']}>
          {list.data!.map((b) => (
            <tr key={b.id}>
              <Td className="font-medium">{b.employee_name}</Td>
              <Td className="font-mono text-xs">{b.matricule}</Td>
              <Td className="font-mono text-xs">{b.badge_code}</Td>
              <Td>
                <Button
                  variant="ghost"
                  aria-label={`Retirer le badge ${b.badge_code}`}
                  onClick={() => del.mutate(b.id)}
                >
                  <Trash2 className="h-4 w-4" aria-hidden />
                </Button>
              </Td>
            </tr>
          ))}
        </Table>
      )}
    </Card>
  )
}

// ---------- Import CSV ----------

function OutcomeTable({ rows }: { rows: PunchOutcome[] }) {
  return (
    <Table head={['Ligne', 'Identifiant', 'Employé', 'Résultat', 'Détail']}>
      {rows.slice(0, 200).map((r, i) => (
        <tr key={i}>
          <Td>{r.line ?? '—'}</Td>
          <Td className="font-mono text-xs">{r.identifier}</Td>
          <Td>{r.employee_name ?? '—'}</Td>
          <Td>
            <StatusTag status={r.status} />
          </Td>
          <Td className="text-xs text-slate-500">{r.reason ?? ''}</Td>
        </tr>
      ))}
    </Table>
  )
}

function CountsRow({ r }: { r: IngestResult }) {
  return (
    <div className="grid gap-3 sm:grid-cols-5">
      <Kpi label="Entrées créées" value={r.counts.entries_opened} />
      <Kpi label="Sorties" value={r.counts.entries_closed} />
      <Kpi label="Doublons" value={r.counts.duplicates} />
      <Kpi label="Badges inconnus" value={r.counts.unknown_badges} />
      <Kpi label="Refusés" value={r.counts.rejected} />
    </div>
  )
}

function ImportCsv() {
  const qc = useQueryClient()
  const [content, setContent] = useState('')
  const [fileName, setFileName] = useState('')
  const [preview, setPreview] = useState<IngestResult | null>(null)
  const [done, setDone] = useState<IngestResult | null>(null)
  const run = useMutation({
    mutationFn: (dry: boolean) => api.post<IngestResult>('/attendance/import', { content, dry_run: dry }),
    onSuccess: async (r) => {
      if (r.dry_run) setPreview(r)
      else {
        setDone(r)
        setPreview(null)
        await qc.invalidateQueries({ queryKey: ['attendance'] })
      }
    },
  })
  async function pick(file: File | undefined) {
    setPreview(null)
    setDone(null)
    run.reset()
    if (!file) return
    setFileName(file.name)
    setContent(await file.text())
  }
  return (
    <Card title="Importer un export de badgeuse (CSV)">
      <p className="mb-4 max-w-3xl text-sm text-slate-500">
        Colonnes reconnues : <code>badge</code> ou <code>matricule</code>, <code>date_heure</code> (ou{' '}
        <code>date</code> + <code>heure</code>), et <code>sens</code> facultatif (entrée/sortie). Les heures
        sans fuseau sont lues dans le fuseau de la société, les dates au format JJ/MM/AAAA ou AAAA-MM-JJ.
        Séparateur « ; », « , » ou tabulation. 2 000 lignes maximum.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800">
          <Upload className="h-4 w-4" aria-hidden /> Choisir un fichier
          <input
            type="file"
            accept=".csv,.txt,text/csv"
            className="sr-only"
            onChange={(e) => pick(e.target.files?.[0])}
            aria-label="Fichier CSV de badgeuse"
          />
        </label>
        {fileName && <span className="text-sm text-slate-500">{fileName}</span>}
        <Button
          variant="secondary"
          disabled={!content}
          loading={run.isPending}
          onClick={() => run.mutate(true)}
        >
          Simuler
        </Button>
        <Button
          disabled={!preview || preview.counts.total === 0}
          loading={run.isPending}
          onClick={() => run.mutate(false)}
        >
          Importer
        </Button>
      </div>
      {run.isError && (
        <div className="mt-4">
          <ErrorBox error={run.error} />
        </div>
      )}
      {preview && (
        <div className="mt-5 space-y-4">
          <p className="text-sm font-medium">
            Simulation — rien n’est enregistré tant que vous n’avez pas cliqué sur « Importer ».
          </p>
          <CountsRow r={preview} />
          <OutcomeTable rows={preview.rows} />
        </div>
      )}
      {done && (
        <div className="mt-5 space-y-4" role="status">
          <p className="text-sm font-medium text-emerald-700 dark:text-emerald-400">Import terminé.</p>
          <CountsRow r={done} />
          {done.counts.unknown_badges + done.counts.rejected > 0 && (
            <OutcomeTable rows={done.rows.filter((r) => ['unknown_badge', 'rejected'].includes(r.status))} />
          )}
        </div>
      )}
    </Card>
  )
}

// ---------- Journal des passages ----------

function Punches() {
  const { me } = useAuth()
  const qc = useQueryClient()
  const [status, setStatus] = useState('')
  const q = useQuery({
    queryKey: ['attendance', 'punches', me?.active_tenant_id, status],
    queryFn: () => api.get<RawPunch[]>(`/attendance/punches${qs({ status })}`),
  })
  const reprocess = useMutation({
    mutationFn: () => api.post<IngestResult>('/attendance/punches/reprocess'),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['attendance'] }),
  })
  return (
    <Card
      title="Journal des passages"
      actions={
        <div className="flex items-center gap-2">
          <Select
            aria-label="Filtrer par résultat"
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            className="w-48"
          >
            <option value="">Tous les résultats</option>
            {Object.entries(STATUS).map(([k, [l]]) => (
              <option key={k} value={k}>
                {l}
              </option>
            ))}
          </Select>
          <Button variant="secondary" loading={reprocess.isPending} onClick={() => reprocess.mutate()}>
            Retraiter les inconnus/refusés
          </Button>
        </div>
      }
    >
      {reprocess.isSuccess && (
        <p className="mb-3 text-sm text-emerald-700 dark:text-emerald-400" role="status">
          Retraitement : {reprocess.data.counts.entries_opened + reprocess.data.counts.entries_closed}{' '}
          passage(s) appliqué(s) sur {reprocess.data.counts.total}.
        </p>
      )}
      {reprocess.isError && <ErrorBox error={reprocess.error} />}
      {q.isLoading ? (
        <Skeleton rows={3} />
      ) : q.isError ? (
        <ErrorBox error={q.error} />
      ) : q.data!.length === 0 ? (
        <EmptyState
          title="Aucun passage reçu"
          hint="Les passages envoyés par une badgeuse ou importés apparaissent ici, avec leur résultat."
        />
      ) : (
        <Table head={['Date et heure', 'Origine', 'Identifiant', 'Employé', 'Résultat', 'Détail']}>
          {q.data!.map((p) => (
            <tr key={p.id}>
              <Td className="tabular-nums">{dateTime(p.punched_at)}</Td>
              <Td>{p.source === 'device' ? (p.device_name ?? 'Badgeuse') : 'Import CSV'}</Td>
              <Td className="font-mono text-xs">{p.identifier}</Td>
              <Td>{p.employee_name ?? '—'}</Td>
              <Td>
                <StatusTag status={p.status} />
              </Td>
              <Td className="text-xs text-slate-500">{p.reason ?? ''}</Td>
            </tr>
          ))}
        </Table>
      )}
    </Card>
  )
}

export default function TimeclockPanel() {
  return (
    <div className="space-y-6">
      <ImportCsv />
      <Devices />
      <Badges />
      <Punches />
    </div>
  )
}
