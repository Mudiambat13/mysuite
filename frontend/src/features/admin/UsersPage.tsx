import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import type { PasswordResetIssued, UserRow } from '../../lib/types'
import {
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  Modal,
  PageHeader,
  Select,
  Skeleton,
  Table,
  Td,
} from '../../components/ui'

const ROLES = ['hr_admin', 'hr_manager', 'payroll_manager', 'manager', 'employee', 'auditor']

export default function UsersPage() {
  const { me } = useAuth()
  const { t } = useTranslation()
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['users', me?.active_tenant_id],
    queryFn: () => api.get<UserRow[]>('/users'),
  })
  const { register, handleSubmit, reset } = useForm<{
    email: string
    full_name: string
    password: string
    role: string
    employee_id: string
  }>({ defaultValues: { role: 'employee' } })
  const create = useMutation({
    mutationFn: (v: {
      email: string
      full_name: string
      password: string
      role: string
      employee_id: string
    }) => api.post('/users', { ...v, employee_id: v.employee_id || null }),
    onSuccess: async () => {
      reset({ role: 'employee' })
      await qc.invalidateQueries({ queryKey: ['users'] })
    },
  })
  const setRole = useMutation({
    mutationFn: (v: { id: string; role: string }) => api.put(`/users/${v.id}/role`, { role: v.role }),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: ['users'] })
    },
  })
  const [issued, setIssued] = useState<{ name: string; link: string; expires: string; emailed: boolean } | null>(null)
  const [notice, setNotice] = useState('')
  const issue = useMutation({
    mutationFn: (u: UserRow) =>
      api.post<PasswordResetIssued>(`/users/${u.user_id}/password-reset`).then((r) => ({ u, r })),
    onSuccess: ({ u, r }) =>
      setIssued({ name: u.full_name, link: `${window.location.origin}${r.path}`, expires: r.expires_at, emailed: r.emailed }),
  })
  const resetMfa = useMutation({
    mutationFn: (u: UserRow) => api.post(`/users/${u.user_id}/mfa-reset`).then(() => u),
    onSuccess: (u) => setNotice(`MFA retiré pour ${u.full_name} ; ses sessions ont été fermées.`),
  })
  return (
    <>
      <PageHeader title="Utilisateurs" subtitle="Comptes et rôles de la société active" />
      <div className="space-y-6">
        <Card title="Comptes">
          {(setRole.isError || issue.isError || resetMfa.isError) && (
            <div className="mb-3">
              <ErrorBox error={setRole.error ?? issue.error ?? resetMfa.error} />
            </div>
          )}
          {notice && (
            <p
              role="status"
              className="mb-3 rounded-lg bg-emerald-50 p-3 text-sm text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200"
            >
              {notice}
            </p>
          )}
          {list.isLoading ? (
            <Skeleton />
          ) : list.isError ? (
            <ErrorBox error={list.error} />
          ) : list.data!.length === 0 ? (
            <EmptyState title="Aucun utilisateur" />
          ) : (
            <Table head={['Nom', 'Email', 'Rôle', 'Accès']}>
              {list.data!.map((u) => (
                <tr key={u.user_id}>
                  <Td className="font-medium">{u.full_name}</Td>
                  <Td>{u.email}</Td>
                  <Td>
                    <Select
                      aria-label={`Rôle de ${u.full_name}`}
                      className="w-48"
                      value={u.role}
                      disabled={u.user_id === me?.user.id}
                      onChange={(e) => setRole.mutate({ id: u.user_id, role: e.target.value })}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {t(`role.${r}`)}
                        </option>
                      ))}
                    </Select>
                  </Td>
                  <Td>
                    {u.user_id !== me?.user.id && (
                      <div className="flex flex-wrap gap-2">
                        <Button
                          variant="secondary"
                          aria-label={`Lien de réinitialisation pour ${u.full_name}`}
                          onClick={() => {
                            setNotice('')
                            issue.mutate(u)
                          }}
                        >
                          Lien de réinitialisation
                        </Button>
                        <Button
                          variant="ghost"
                          aria-label={`Réinitialiser le MFA de ${u.full_name}`}
                          onClick={() => {
                            setNotice('')
                            if (window.confirm(`Retirer le MFA de ${u.full_name} ?`)) resetMfa.mutate(u)
                          }}
                        >
                          Réinitialiser le MFA
                        </Button>
                      </div>
                    )}
                  </Td>
                </tr>
              ))}
            </Table>
          )}
        </Card>
        <Card title="Nouvel utilisateur">
          <form
            onSubmit={handleSubmit((v) => create.mutate(v))}
            className="grid items-end gap-3 sm:grid-cols-3"
          >
            {create.isError && (
              <div className="sm:col-span-3">
                <ErrorBox error={create.error} />
              </div>
            )}
            <Field label="Nom complet">{(id) => <Input id={id} required {...register('full_name')} />}</Field>
            <Field label="Email">
              {(id) => <Input id={id} type="email" required {...register('email')} />}
            </Field>
            <Field label="Mot de passe initial" hint="12 caractères minimum, lettres et chiffres">
              {(id) => (
                <Input
                  id={id}
                  type="password"
                  autoComplete="new-password"
                  required
                  {...register('password')}
                />
              )}
            </Field>
            <Field label="Rôle">
              {(id) => (
                <Select id={id} {...register('role')}>
                  {ROLES.map((r) => (
                    <option key={r} value={r}>
                      {t(`role.${r}`)}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label="ID du dossier employé (facultatif)">
              {(id) => <Input id={id} {...register('employee_id')} />}
            </Field>
            <Button type="submit" loading={create.isPending}>
              Créer
            </Button>
          </form>
        </Card>
      </div>
      {issued && (
        <Modal title="Lien de réinitialisation" onClose={() => setIssued(null)}>
          <div className="space-y-3">
            <p className="text-sm text-slate-600 dark:text-slate-300">
              {issued.emailed
                ? `Le lien a aussi été envoyé par courriel à ${issued.name}. `
                : 'Aucun courriel envoyé (SMTP non configuré ou envoi échoué). '}
              Transmettez ce lien à {issued.name} par un canal sûr si besoin. Il n'est affiché qu'une fois, ne sert
              qu'une fois et expire le {new Date(issued.expires).toLocaleString('fr-FR')}.
            </p>
            <p
              className="break-all rounded bg-slate-50 p-2 font-mono text-sm dark:bg-slate-800"
              data-testid="reset-link"
            >
              {issued.link}
            </p>
            <Button
              variant="secondary"
              onClick={() => navigator.clipboard?.writeText(issued.link).catch(() => undefined)}
            >
              Copier le lien
            </Button>
          </div>
        </Modal>
      )}
    </>
  )
}
