import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { useMutation } from '@tanstack/react-query'
import ThemeSwitch from '../../components/ThemeSwitch'
import { Navigate, useNavigate } from 'react-router-dom'
import { api, ApiError } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { Button, ErrorBox, Field, Input } from '../../components/ui'
import { needsMfa, type LoginResult } from '../../lib/types'

const schema = z.object({
  email: z.string().min(1, 'Email requis').email('Email invalide'),
  password: z.string().min(1, 'Mot de passe requis'),
})
type Values = z.infer<typeof schema>

export default function LoginPage() {
  const { me, loading, refresh } = useAuth()
  const nav = useNavigate()
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<Values>({ resolver: zodResolver(schema) })
  const [challenge, setChallenge] = useState<string | null>(null)
  const [code, setCode] = useState('')
  const done = async () => {
    await refresh()
    nav('/')
  }
  const login = useMutation({
    mutationFn: (v: Values) => api.post<LoginResult>('/auth/login', v),
    onSuccess: async (r) => {
      if (needsMfa(r)) setChallenge(r.mfa_token)
      else await done()
    },
  })
  const second = useMutation({
    mutationFn: () => api.post<LoginResult>('/auth/login/mfa', { mfa_token: challenge, code }),
    onSuccess: done,
    onError: (e) => {
      // Défi expiré, épuisé ou compte verrouillé : retour à la saisie du mot de passe.
      if (e instanceof ApiError && e.status !== 401) setChallenge(null)
    },
  })
  if (!loading && me) return <Navigate to="/" replace />

  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <div className="mx-auto mb-3 flex h-12 w-12 items-center justify-center rounded-xl bg-brand-600 text-xl font-bold text-white">
            M
          </div>
          <h1 className="text-2xl font-semibold tracking-tight">MySuite</h1>
          <p className="mt-1 text-sm text-slate-500">Connectez-vous à votre espace RH</p>
        </div>
        <div className="mb-4 flex justify-center">
          <ThemeSwitch />
        </div>
        {challenge ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              second.mutate()
            }}
            className="space-y-4 rounded-xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900"
          >
            <p className="text-sm text-slate-600 dark:text-slate-300">
              Saisissez le code à 6 chiffres de votre application d'authentification, ou un code de secours.
            </p>
            {second.isError && <ErrorBox error={second.error} />}
            <Field label="Code de vérification">
              {(id) => (
                <Input
                  id={id}
                  value={code}
                  onChange={(e) => setCode(e.target.value)}
                  inputMode="text"
                  autoComplete="one-time-code"
                  autoFocus
                  required
                />
              )}
            </Field>
            <Button type="submit" loading={second.isPending} className="w-full">
              Vérifier
            </Button>
            <Button
              type="button"
              variant="ghost"
              className="w-full"
              onClick={() => {
                setChallenge(null)
                setCode('')
              }}
            >
              Retour
            </Button>
          </form>
        ) : (
          <form
            onSubmit={handleSubmit((v) => login.mutate(v))}
            className="space-y-4 rounded-xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900"
            noValidate
          >
            {login.isError && <ErrorBox error={login.error} />}
            <Field label="Adresse email" error={errors.email?.message}>
              {(id) => <Input id={id} type="email" autoComplete="username" {...register('email')} />}
            </Field>
            <Field label="Mot de passe" error={errors.password?.message}>
              {(id) => (
                <Input id={id} type="password" autoComplete="current-password" {...register('password')} />
              )}
            </Field>
            <Button type="submit" loading={login.isPending} className="w-full">
              Se connecter
            </Button>
            <p className="text-center text-xs text-slate-500">
              Mot de passe oublié ? Demandez un lien de réinitialisation à votre administrateur RH.
            </p>
          </form>
        )}
      </div>
    </div>
  )
}
