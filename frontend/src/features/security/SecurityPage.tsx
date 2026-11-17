import { useEffect, useState } from 'react'
import QRCode from 'qrcode'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import type { MfaSetup, MfaStatus, RecoveryCodes } from '../../lib/types'
import { Badge, Button, Card, ErrorBox, Field, Input, PageHeader, Skeleton } from '../../components/ui'

function RecoveryList({ codes, onDone }: { codes: string[]; onDone: () => void }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="space-y-3" data-testid="recovery-codes">
      <p className="text-sm text-slate-600 dark:text-slate-300">
        Conservez ces codes de secours dans un endroit sûr. Chacun ne sert qu'une seule fois et ils ne seront
        plus affichés.
      </p>
      <ul className="grid grid-cols-2 gap-2 rounded-lg bg-slate-50 p-3 font-mono text-sm dark:bg-slate-800">
        {codes.map((c) => (
          <li key={c}>{c}</li>
        ))}
      </ul>
      <div className="flex gap-2">
        <Button
          variant="secondary"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(codes.join('\n'))
              setCopied(true)
            } catch {
              setCopied(false)
            }
          }}
        >
          {copied ? 'Copié' : 'Copier'}
        </Button>
        <Button onClick={onDone}>J'ai enregistré mes codes</Button>
      </div>
    </div>
  )
}

function QrCode({ uri }: { uri: string }) {
  const [svg, setSvg] = useState('')
  useEffect(() => {
    let alive = true
    QRCode.toString(uri, { type: 'svg', margin: 1, width: 192 })
      .then((s) => alive && setSvg(s))
      .catch(() => alive && setSvg(''))
    return () => {
      alive = false
    }
  }, [uri])
  if (!svg) return null
  return (
    <div
      className="inline-block rounded-lg bg-white p-2"
      role="img"
      aria-label="QR code de configuration"
      // SVG généré localement par la bibliothèque à partir de l'URI ; aucune donnée externe.
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  )
}

function PolicyCard() {
  const qc = useQueryClient()
  const q = useQuery({
    queryKey: ['mfa-policy'],
    queryFn: () => api.get<{ mfa_required: boolean }>('/security/policy'),
  })
  const set = useMutation({
    mutationFn: (v: boolean) => api.put<{ mfa_required: boolean }>('/security/policy', { mfa_required: v }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['mfa-policy'] }),
  })
  if (q.isLoading) return <Skeleton />
  if (q.isError) return <ErrorBox error={q.error} />
  return (
    <Card title="Politique de la société">
      {set.isError && <ErrorBox error={set.error} />}
      <label className="flex items-start gap-3 text-sm">
        <input
          type="checkbox"
          className="mt-1"
          checked={q.data!.mfa_required}
          disabled={set.isPending}
          onChange={(e) => set.mutate(e.target.checked)}
        />
        <span>
          <span className="font-medium">
            Exiger l'authentification à deux facteurs pour tous les membres.
          </span>
          <span className="block text-slate-500">
            Un membre sans MFA ne peut alors que configurer son MFA. Vous devez avoir activé le vôtre avant
            d'imposer la règle.
          </span>
        </span>
      </label>
    </Card>
  )
}

export default function SecurityPage() {
  const qc = useQueryClient()
  const { me, can, refresh: refreshMe } = useAuth()
  const status = useQuery({ queryKey: ['mfa'], queryFn: () => api.get<MfaStatus>('/auth/mfa') })
  const [setupPwd, setSetupPwd] = useState('')
  const [setup, setSetup] = useState<MfaSetup | null>(null)
  const [enableCode, setEnableCode] = useState('')
  const [codes, setCodes] = useState<string[] | null>(null)
  const [pwd, setPwd] = useState('')
  const [code, setCode] = useState('')

  const refresh = async () => {
    await qc.invalidateQueries({ queryKey: ['mfa'] })
    await refreshMe()
  }
  const start = useMutation({
    mutationFn: () => api.post<MfaSetup>('/auth/mfa/setup', { password: setupPwd }),
    onSuccess: (r) => {
      setSetup(r)
      setSetupPwd('')
    },
  })
  const enable = useMutation({
    mutationFn: () => api.post<RecoveryCodes>('/auth/mfa/enable', { code: enableCode }),
    onSuccess: async (r) => {
      setCodes(r.recovery_codes)
      setSetup(null)
      setEnableCode('')
      await refresh()
    },
  })
  const regen = useMutation({
    mutationFn: () => api.post<RecoveryCodes>('/auth/mfa/recovery-codes', { password: pwd, code }),
    onSuccess: async (r) => {
      setCodes(r.recovery_codes)
      setPwd('')
      setCode('')
      await refresh()
    },
  })
  const disable = useMutation({
    mutationFn: () => api.post('/auth/mfa/disable', { password: pwd, code }),
    onSuccess: async () => {
      setPwd('')
      setCode('')
      await refresh()
    },
  })

  const s = status.data
  return (
    <>
      <PageHeader
        title="Sécurité du compte"
        subtitle="Authentification à deux facteurs (application d'authentification, TOTP)"
      />
      <div className="max-w-2xl space-y-6">
        {me?.mfa_enrollment_required && (
          <p
            role="alert"
            className="rounded-lg bg-amber-50 p-3 text-sm text-amber-900 dark:bg-amber-950 dark:text-amber-200"
          >
            Votre société exige l'authentification à deux facteurs. Activez-la pour accéder au reste de
            MySuite.
          </p>
        )}
        {can('users_manage') && <PolicyCard />}
        {status.isLoading ? (
          <Skeleton />
        ) : status.isError ? (
          <ErrorBox error={status.error} />
        ) : (
          <Card title="Authentification à deux facteurs">
            <div className="mb-4 flex items-center gap-2">
              <Badge tone={s!.enabled ? 'green' : 'slate'}>{s!.enabled ? 'Activée' : 'Désactivée'}</Badge>
              {s!.enabled && (
                <span className="text-sm text-slate-500">
                  {s!.recovery_codes_remaining} code(s) de secours restant(s)
                </span>
              )}
            </div>

            {codes ? (
              <RecoveryList codes={codes} onDone={() => setCodes(null)} />
            ) : !s!.available ? (
              <p className="text-sm text-slate-600 dark:text-slate-300">
                Le serveur n'est pas configuré pour le MFA : l'administrateur doit définir la variable
                MFA_ENCRYPTION_KEY.
              </p>
            ) : !s!.enabled ? (
              setup ? (
                <form
                  className="space-y-4"
                  onSubmit={(e) => {
                    e.preventDefault()
                    enable.mutate()
                  }}
                >
                  <ol className="list-decimal space-y-1 pl-5 text-sm text-slate-600 dark:text-slate-300">
                    <li>Scannez le QR code avec votre application d'authentification.</li>
                    <li>Ou saisissez la clé manuellement.</li>
                    <li>Entrez le code à 6 chiffres affiché pour confirmer.</li>
                  </ol>
                  <QrCode uri={setup.otpauth_uri} />
                  <p className="break-all rounded bg-slate-50 p-2 font-mono text-sm dark:bg-slate-800">
                    {setup.secret}
                  </p>
                  {enable.isError && <ErrorBox error={enable.error} />}
                  <Field label="Code à 6 chiffres">
                    {(id) => (
                      <Input
                        id={id}
                        value={enableCode}
                        onChange={(e) => setEnableCode(e.target.value)}
                        inputMode="numeric"
                        autoComplete="one-time-code"
                        required
                      />
                    )}
                  </Field>
                  <div className="flex gap-2">
                    <Button type="submit" loading={enable.isPending}>
                      Activer
                    </Button>
                    <Button type="button" variant="ghost" onClick={() => setSetup(null)}>
                      Annuler
                    </Button>
                  </div>
                </form>
              ) : (
                <form
                  className="space-y-4"
                  onSubmit={(e) => {
                    e.preventDefault()
                    start.mutate()
                  }}
                >
                  <p className="text-sm text-slate-600 dark:text-slate-300">
                    Ajoute un code à usage unique à votre connexion. Confirmez d'abord votre mot de passe.
                  </p>
                  {start.isError && <ErrorBox error={start.error} />}
                  <Field label="Mot de passe actuel">
                    {(id) => (
                      <Input
                        id={id}
                        type="password"
                        autoComplete="current-password"
                        value={setupPwd}
                        onChange={(e) => setSetupPwd(e.target.value)}
                        required
                      />
                    )}
                  </Field>
                  <Button type="submit" loading={start.isPending}>
                    Configurer le MFA
                  </Button>
                </form>
              )
            ) : (
              <form className="space-y-4" onSubmit={(e) => e.preventDefault()}>
                <p className="text-sm text-slate-600 dark:text-slate-300">
                  Pour régénérer les codes de secours ou désactiver le MFA, confirmez votre mot de passe et un
                  code (application ou secours).
                </p>
                {(regen.isError || disable.isError) && <ErrorBox error={regen.error ?? disable.error} />}
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field label="Mot de passe actuel">
                    {(id) => (
                      <Input
                        id={id}
                        type="password"
                        autoComplete="current-password"
                        value={pwd}
                        onChange={(e) => setPwd(e.target.value)}
                      />
                    )}
                  </Field>
                  <Field label="Code de vérification">
                    {(id) => (
                      <Input
                        id={id}
                        value={code}
                        onChange={(e) => setCode(e.target.value)}
                        autoComplete="one-time-code"
                      />
                    )}
                  </Field>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    loading={regen.isPending}
                    disabled={!pwd || !code}
                    onClick={() => regen.mutate()}
                  >
                    Régénérer les codes de secours
                  </Button>
                  <Button
                    type="button"
                    variant="danger"
                    loading={disable.isPending}
                    disabled={!pwd || !code}
                    onClick={() => disable.mutate()}
                  >
                    Désactiver le MFA
                  </Button>
                </div>
              </form>
            )}
          </Card>
        )}
      </div>
    </>
  )
}
