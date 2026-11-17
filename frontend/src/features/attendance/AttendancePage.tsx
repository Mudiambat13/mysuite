import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, LogIn, LogOut, Pencil, Plus } from 'lucide-react'
import { api, qs } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { dateInZone, hoursMinutes, isoToZonedInput, timeInZone, zonedInputToIso } from '../../lib/format'
import type {
  AttendanceEntry,
  AttendanceSettings,
  AttendanceSummary,
  Employee,
  MyAttendance,
  Page,
} from '../../lib/types'
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  Kpi,
  Modal,
  PageHeader,
  Skeleton,
  Table,
  Td,
  cx,
} from '../../components/ui'
import { Select } from '../../components/ui'
import TimeclockPanel from './TimeclockPanel'

type Tab = 'mine' | 'team' | 'clock' | 'settings'

function useTick(ms: number) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), ms)
    return () => clearInterval(id)
  }, [ms])
  return now
}

function EntryRow({ e, tz, action }: { e: AttendanceEntry; tz: string; action?: React.ReactNode }) {
  return (
    <tr>
      <Td>{dateInZone(e.clock_in, tz)}</Td>
      <Td className="tabular-nums">{timeInZone(e.clock_in, tz)}</Td>
      <Td className="tabular-nums">
        {e.clock_out ? timeInZone(e.clock_out, tz) : <Badge tone="amber">En cours</Badge>}
      </Td>
      <Td className="tabular-nums">{hoursMinutes(e.minutes)}</Td>
      <Td>
        {e.source === 'manual' ? (
          <Badge tone="blue">Saisie RH</Badge>
        ) : e.source === 'device' ? (
          <Badge tone="green">Badgeuse</Badge>
        ) : e.source === 'import' ? (
          <Badge tone="green">Import</Badge>
        ) : (
          <Badge>Pointage</Badge>
        )}
        {e.corrected && <span className="ml-1 text-xs text-slate-500">(corrigé)</span>}
      </Td>
      {action !== undefined && <Td>{action}</Td>}
    </tr>
  )
}

// ---------- Mon pointage ----------

function MyClock() {
  const { me } = useAuth()
  const qc = useQueryClient()
  const key = ['attendance', 'me', me?.active_tenant_id]
  const q = useQuery({ queryKey: key, queryFn: () => api.get<MyAttendance>('/attendance/me'), retry: false })
  const now = useTick(30_000)
  const punch = useMutation({
    mutationFn: (kind: 'clock-in' | 'clock-out') => api.post(`/attendance/${kind}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['attendance'] }),
  })
  if (q.isLoading) return <Skeleton rows={3} />
  if (q.isError)
    return (
      <Card>
        <EmptyState
          title="Pointage indisponible pour ce compte"
          hint="Votre compte n'est lié à aucun dossier employé : demandez aux RH de le lier."
        />
      </Card>
    )
  const d = q.data!
  const tz = d.settings.timezone
  const open = d.open
  const elapsed = open ? Math.max(0, Math.floor((now - new Date(open.clock_in).getTime()) / 60000)) : 0
  return (
    <div className="space-y-6">
      <Card>
        <div className="flex flex-col items-center gap-4 py-4 text-center">
          <p className="text-sm text-slate-500">
            {open ? `En poste depuis ${timeInZone(open.clock_in, tz)}` : 'Vous n’êtes pas en poste'}
          </p>
          <p className="text-4xl font-semibold tabular-nums" aria-live="polite">
            {open ? hoursMinutes(elapsed) : hoursMinutes(d.today_minutes)}
          </p>
          <Button
            className="min-w-56 py-3 text-base"
            variant={open ? 'danger' : 'primary'}
            loading={punch.isPending}
            onClick={() => punch.mutate(open ? 'clock-out' : 'clock-in')}
          >
            {open ? <LogOut className="h-5 w-5" aria-hidden /> : <LogIn className="h-5 w-5" aria-hidden />}
            {open ? 'Pointer la sortie' : 'Pointer l’entrée'}
          </Button>
          {punch.isError && <ErrorBox error={punch.error} />}
          <p className="text-xs text-slate-500">L’heure enregistrée est celle du serveur (fuseau {tz}).</p>
        </div>
      </Card>
      <Card title="Aujourd’hui">
        {d.today.length === 0 ? (
          <EmptyState title="Aucun pointage aujourd’hui" />
        ) : (
          <Table head={['Date', 'Entrée', 'Sortie', 'Durée', 'Origine']}>
            {d.today.map((e) => (
              <EntryRow key={e.id} e={e} tz={tz} />
            ))}
          </Table>
        )}
      </Card>
      <MyHistory tz={tz} />
    </div>
  )
}

function MyHistory({ tz }: { tz: string }) {
  const { me } = useAuth()
  const q = useQuery({
    queryKey: ['attendance', 'mine-list', me?.active_tenant_id],
    queryFn: () => api.get<AttendanceEntry[]>('/attendance/entries?scope=mine'),
  })
  return (
    <Card title="Mon historique du mois">
      {q.isLoading ? (
        <Skeleton rows={3} />
      ) : q.isError ? (
        <ErrorBox error={q.error} />
      ) : q.data!.length === 0 ? (
        <EmptyState title="Aucun pointage ce mois-ci" />
      ) : (
        <Table head={['Date', 'Entrée', 'Sortie', 'Durée', 'Origine']}>
          {q.data!.map((e) => (
            <EntryRow key={e.id} e={e} tz={tz} />
          ))}
        </Table>
      )}
    </Card>
  )
}

// ---------- Équipe ----------

function EntryModal({
  tz,
  entry,
  onClose,
}: {
  tz: string
  entry: AttendanceEntry | null
  onClose: () => void
}) {
  const qc = useQueryClient()
  const [employee, setEmployee] = useState('')
  const [from, setFrom] = useState(entry ? isoToZonedInput(entry.clock_in, tz) : '')
  const [to, setTo] = useState(entry?.clock_out ? isoToZonedInput(entry.clock_out, tz) : '')
  const [note, setNote] = useState('')
  const emps = useQuery({
    queryKey: ['attendance', 'employees'],
    queryFn: () => api.get<Page<Employee>>('/employees?limit=100&status=active'),
    enabled: !entry,
  })
  const save = useMutation({
    mutationFn: () => {
      const body = {
        clock_in: zonedInputToIso(from, tz),
        clock_out: to ? zonedInputToIso(to, tz) : null,
        note,
      }
      return entry
        ? api.put(`/attendance/entries/${entry.id}`, body)
        : api.post('/attendance/entries', { ...body, employee_id: employee })
    },
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['attendance'] })
      onClose()
    },
  })
  const valid = from && note.trim() && (entry || employee)
  return (
    <Modal
      title={entry ? `Corriger le pointage de ${entry.employee_name}` : 'Saisir un pointage'}
      onClose={onClose}
    >
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault()
          if (valid) save.mutate()
        }}
      >
        {!entry && (
          <Field label="Employé">
            {(id) => (
              <Select id={id} value={employee} onChange={(e) => setEmployee(e.target.value)} required>
                <option value="">Choisir…</option>
                {emps.data?.items.map((e) => (
                  <option key={e.id} value={e.id}>
                    {e.matricule} — {e.first_name} {e.last_name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        )}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Entrée" hint={`Heure locale (${tz})`}>
            {(id) => (
              <Input
                id={id}
                type="datetime-local"
                value={from}
                onChange={(e) => setFrom(e.target.value)}
                required
              />
            )}
          </Field>
          <Field label="Sortie" hint="Laisser vide si le pointage reste ouvert">
            {(id) => (
              <Input id={id} type="datetime-local" value={to} onChange={(e) => setTo(e.target.value)} />
            )}
          </Field>
        </div>
        <Field label="Motif (obligatoire, conservé dans le journal d’audit)">
          {(id) => (
            <Input id={id} value={note} onChange={(e) => setNote(e.target.value)} required maxLength={300} />
          )}
        </Field>
        {save.isError && <ErrorBox error={save.error} />}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" onClick={onClose}>
            Annuler
          </Button>
          <Button type="submit" loading={save.isPending} disabled={!valid}>
            Enregistrer
          </Button>
        </div>
      </form>
    </Modal>
  )
}

function TeamView() {
  const { me, can } = useAuth()
  const now = new Date()
  const [from, setFrom] = useState(`${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-01`)
  const [to, setTo] = useState(now.toISOString().slice(0, 10))
  const [selected, setSelected] = useState<string | null>(null)
  const [modal, setModal] = useState<{ entry: AttendanceEntry | null } | null>(null)
  const params = { from, to, scope: 'team' }
  const sum = useQuery({
    queryKey: ['attendance', 'summary', me?.active_tenant_id, from, to],
    queryFn: () => api.get<AttendanceSummary>(`/attendance/summary${qs(params)}`),
  })
  const entries = useQuery({
    queryKey: ['attendance', 'entries', me?.active_tenant_id, from, to, selected],
    queryFn: () =>
      api.get<AttendanceEntry[]>(`/attendance/entries${qs({ ...params, employee_id: selected })}`),
    enabled: !!selected,
  })
  const tz = sum.data?.settings.timezone ?? 'UTC'
  const manage = can('attendance_manage')
  const rows = sum.data?.rows ?? []
  const total = (f: (r: AttendanceSummary['rows'][number]) => number) => rows.reduce((n, r) => n + f(r), 0)
  const picked = rows.find((r) => r.employee_id === selected)
  return (
    <div className="space-y-6">
      <Card>
        <div className="flex flex-wrap items-end gap-3">
          <div className="w-44">
            <Field label="Du">
              {(id) => <Input id={id} type="date" value={from} onChange={(e) => setFrom(e.target.value)} />}
            </Field>
          </div>
          <div className="w-44">
            <Field label="Au">
              {(id) => <Input id={id} type="date" value={to} onChange={(e) => setTo(e.target.value)} />}
            </Field>
          </div>
          <div className="ml-auto flex gap-2">
            {manage && (
              <Button variant="secondary" onClick={() => setModal({ entry: null })}>
                <Plus className="h-4 w-4" aria-hidden /> Saisir un pointage
              </Button>
            )}
            <a
              href={`/api/v1/attendance/summary.csv${qs(params)}`}
              className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
            >
              <Download className="h-4 w-4" aria-hidden /> Exporter CSV
            </a>
          </div>
        </div>
      </Card>
      {sum.isLoading ? (
        <Skeleton rows={4} />
      ) : sum.isError ? (
        <ErrorBox error={sum.error} />
      ) : (
        <>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Kpi label="Employés présents" value={rows.length} />
            <Kpi label="Heures travaillées" value={hoursMinutes(total((r) => r.worked_minutes))} />
            <Kpi
              label="Jours en retard"
              value={total((r) => r.late_days)}
              hint={`Après ${sum.data!.settings.work_start.slice(0, 5)} + ${sum.data!.settings.grace_minutes} min`}
            />
            <Kpi
              label="Au-delà de la durée standard"
              value={hoursMinutes(total((r) => r.overtime_minutes))}
              hint="Indicatif, hors paie"
            />
          </div>
          <Card title="Résumé par employé">
            {rows.length === 0 ? (
              <EmptyState title="Aucun pointage sur la période" hint="Élargissez les dates." />
            ) : (
              <Table head={['Matricule', 'Nom', 'Jours', 'Heures', 'Retards', 'Au-delà standard', 'Ouverts']}>
                {rows.map((r) => (
                  <tr
                    key={r.employee_id}
                    className={cx(
                      'cursor-pointer hover:bg-slate-50 dark:hover:bg-slate-800/50',
                      selected === r.employee_id && 'bg-brand-50 dark:bg-slate-800',
                    )}
                    onClick={() => setSelected(selected === r.employee_id ? null : r.employee_id)}
                  >
                    <Td className="font-mono text-xs">{r.matricule}</Td>
                    <Td className="font-medium">
                      <button
                        className="text-left hover:underline"
                        aria-expanded={selected === r.employee_id}
                      >
                        {r.name}
                      </button>
                    </Td>
                    <Td>{r.days_present}</Td>
                    <Td className="tabular-nums">{hoursMinutes(r.worked_minutes)}</Td>
                    <Td>{r.late_days > 0 ? <Badge tone="amber">{r.late_days}</Badge> : 0}</Td>
                    <Td className="tabular-nums">{hoursMinutes(r.overtime_minutes)}</Td>
                    <Td>{r.open_days > 0 ? <Badge tone="red">{r.open_days}</Badge> : 0}</Td>
                  </tr>
                ))}
              </Table>
            )}
          </Card>
          {selected && (
            <Card title={`Détail — ${picked?.name ?? ''}`}>
              {entries.isLoading ? (
                <Skeleton rows={3} />
              ) : entries.isError ? (
                <ErrorBox error={entries.error} />
              ) : (
                <Table head={['Date', 'Entrée', 'Sortie', 'Durée', 'Origine', ...(manage ? [''] : [])]}>
                  {entries.data!.map((e) => (
                    <EntryRow
                      key={e.id}
                      e={e}
                      tz={tz}
                      action={
                        manage ? (
                          e.employee_id === me?.employee_id ? (
                            <span className="text-xs text-slate-400">Votre pointage</span>
                          ) : (
                            <Button
                              variant="ghost"
                              aria-label={`Corriger le pointage du ${dateInZone(e.clock_in, tz)}`}
                              onClick={() => setModal({ entry: e })}
                            >
                              <Pencil className="h-4 w-4" aria-hidden />
                            </Button>
                          )
                        ) : undefined
                      }
                    />
                  ))}
                </Table>
              )}
            </Card>
          )}
        </>
      )}
      {modal && <EntryModal tz={tz} entry={modal.entry} onClose={() => setModal(null)} />}
    </div>
  )
}

// ---------- Paramètres ----------

function SettingsCard() {
  const qc = useQueryClient()
  const q = useQuery({
    queryKey: ['attendance', 'settings'],
    queryFn: () => api.get<AttendanceSettings>('/attendance/settings'),
  })
  const [form, setForm] = useState<AttendanceSettings | null>(null)
  const f = form ?? q.data
  const save = useMutation({
    mutationFn: (v: AttendanceSettings) => api.put<AttendanceSettings>('/attendance/settings', v),
    onSuccess: async () => {
      setForm(null)
      await qc.invalidateQueries({ queryKey: ['attendance'] })
    },
  })
  if (q.isLoading || !f) return <Skeleton rows={3} />
  if (q.isError) return <ErrorBox error={q.error} />
  const set = (patch: Partial<AttendanceSettings>) => setForm({ ...f, ...patch })
  return (
    <Card title="Paramètres du pointage">
      <form
        className="grid max-w-2xl gap-4 sm:grid-cols-2"
        onSubmit={(e) => {
          e.preventDefault()
          save.mutate({ ...f, work_start: f.work_start.length === 5 ? `${f.work_start}:00` : f.work_start })
        }}
      >
        <Field label="Fuseau horaire" hint="Nom IANA, ex. Africa/Lubumbashi">
          {(id) => (
            <Input id={id} value={f.timezone} onChange={(e) => set({ timezone: e.target.value })} required />
          )}
        </Field>
        <Field label="Heure de début de journée">
          {(id) => (
            <Input
              id={id}
              type="time"
              value={f.work_start.slice(0, 5)}
              onChange={(e) => set({ work_start: e.target.value })}
              required
            />
          )}
        </Field>
        <Field label="Tolérance de retard (minutes)">
          {(id) => (
            <Input
              id={id}
              type="number"
              min={0}
              max={240}
              value={f.grace_minutes}
              onChange={(e) => set({ grace_minutes: Number(e.target.value) })}
            />
          )}
        </Field>
        <Field
          label="Durée journalière standard (minutes)"
          hint="Au-delà : minutes indicatives, sans effet sur la paie"
        >
          {(id) => (
            <Input
              id={id}
              type="number"
              min={60}
              max={1440}
              value={f.daily_minutes}
              onChange={(e) => set({ daily_minutes: Number(e.target.value) })}
            />
          )}
        </Field>
        <div className="sm:col-span-2">
          {save.isError && <ErrorBox error={save.error} />}
          {save.isSuccess && !form && (
            <p className="mb-2 text-sm text-emerald-600">Paramètres enregistrés.</p>
          )}
          <Button type="submit" loading={save.isPending} disabled={!form}>
            Enregistrer
          </Button>
        </div>
      </form>
    </Card>
  )
}

export default function AttendancePage() {
  const { can, me } = useAuth()
  const tabs: [Tab, string, boolean][] = [
    ['mine', 'Mon pointage', can('attendance_clock_own')],
    [
      'team',
      can('attendance_manage') || me?.role !== 'manager' ? 'Présences' : 'Mon équipe',
      can('attendance_read'),
    ],
    ['clock', 'Badgeuses', can('attendance_manage')],
    ['settings', 'Paramètres', can('attendance_manage')],
  ]
  const visible = tabs.filter((t) => t[2])
  const [tab, setTab] = useState<Tab>(visible[0]?.[0] ?? 'mine')
  return (
    <>
      <PageHeader title="Pointage" subtitle="Entrées, sorties, retards et heures — heure du serveur" />
      <div
        className="mb-4 flex gap-1 overflow-x-auto border-b border-slate-200 dark:border-slate-800"
        role="tablist"
      >
        {visible.map(([k, label]) => (
          <button
            key={k}
            role="tab"
            aria-selected={tab === k}
            onClick={() => setTab(k)}
            className={cx(
              '-mb-px whitespace-nowrap border-b-2 px-4 py-2 text-sm font-medium',
              tab === k
                ? 'border-brand-600 text-brand-700 dark:text-brand-100'
                : 'border-transparent text-slate-500 hover:text-slate-800',
            )}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === 'mine' && <MyClock />}
      {tab === 'team' && <TeamView />}
      {tab === 'clock' && <TimeclockPanel />}
      {tab === 'settings' && <SettingsCard />}
    </>
  )
}
