import { useState } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { useMutation } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { Button, ErrorBox, Field, Input } from '../../components/ui'

export default function ResetPasswordPage() {
  const [params] = useSearchParams()
  const token = params.get('token') ?? ''
  const [pwd, setPwd] = useState('')
  const [confirm, setConfirm] = useState('')
  const reset = useMutation({
    mutationFn: () => api.post('/auth/reset-password', { token, new_password: pwd }),
  })
  const mismatch = confirm !== '' && pwd !== confirm
  return (
    <div className="flex min-h-screen items-center justify-center p-4">
      <div className="w-full max-w-sm">
        <h1 className="mb-6 text-center text-2xl font-semibold tracking-tight">Nouveau mot de passe</h1>
        <div className="space-y-4 rounded-xl border border-slate-200 bg-white p-6 shadow-sm dark:border-slate-800 dark:bg-slate-900">
          {reset.isSuccess ? (
            <>
              <p role="status" className="text-sm">
                Mot de passe modifié. Toutes vos sessions ont été fermées.
              </p>
              <Link to="/login" className="block text-center text-sm font-medium text-brand-600">
                Se connecter
              </Link>
            </>
          ) : !token ? (
            <p className="text-sm">Lien invalide : demandez-en un nouveau à votre administrateur RH.</p>
          ) : (
            <form
              className="space-y-4"
              onSubmit={(e) => {
                e.preventDefault()
                if (!mismatch) reset.mutate()
              }}
            >
              {reset.isError && <ErrorBox error={reset.error} />}
              <Field label="Nouveau mot de passe" hint="12 caractères minimum, lettres et chiffres">
                {(id) => (
                  <Input
                    id={id}
                    type="password"
                    autoComplete="new-password"
                    value={pwd}
                    onChange={(e) => setPwd(e.target.value)}
                    required
                  />
                )}
              </Field>
              <Field label="Confirmer" error={mismatch ? 'Les mots de passe diffèrent' : undefined}>
                {(id) => (
                  <Input
                    id={id}
                    type="password"
                    autoComplete="new-password"
                    value={confirm}
                    onChange={(e) => setConfirm(e.target.value)}
                    required
                  />
                )}
              </Field>
              <Button type="submit" loading={reset.isPending} className="w-full" disabled={mismatch}>
                Enregistrer
              </Button>
            </form>
          )}
        </div>
      </div>
    </div>
  )
}
