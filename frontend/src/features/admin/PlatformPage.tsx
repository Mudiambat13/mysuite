import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../lib/api'
import type { TenantRow } from '../../lib/types'
import {
  Button,
  Card,
  EmptyState,
  ErrorBox,
  Field,
  Input,
  PageHeader,
  Skeleton,
  StatusBadge,
  Table,
  Td,
} from '../../components/ui'

interface V {
  name: string
  slug: string
  currency: string
  admin_name: string
  admin_email: string
  admin_password: string
}

export default function PlatformPage() {
  const qc = useQueryClient()
  const list = useQuery({
    queryKey: ['platform-tenants'],
    queryFn: () => api.get<TenantRow[]>('/platform/tenants'),
  })
  const { register, handleSubmit, reset } = useForm<V>({ defaultValues: { currency: 'USD' } })
  const create = useMutation({
    mutationFn: (v: V) => api.post('/platform/tenants', v),
    onSuccess: async () => {
      reset({ currency: 'USD' })
      await qc.invalidateQueries({ queryKey: ['platform-tenants'] })
    },
  })
  return (
    <>
      <PageHeader title="Sociétés" subtitle="Administration de la plateforme" />
      <div className="space-y-6">
        <Card title="Sociétés hébergées">
          {list.isLoading ? (
            <Skeleton />
          ) : list.isError ? (
            <ErrorBox error={list.error} />
          ) : list.data!.length === 0 ? (
            <EmptyState title="Aucune société" />
          ) : (
            <Table head={['Nom', 'Identifiant', 'Pays', 'Devise', 'Statut']}>
              {list.data!.map((t) => (
                <tr key={t.id}>
                  <Td className="font-medium">{t.name}</Td>
                  <Td className="font-mono text-xs">{t.slug}</Td>
                  <Td>{t.country}</Td>
                  <Td>{t.currency}</Td>
                  <Td>
                    <StatusBadge status={t.status} />
                  </Td>
                </tr>
              ))}
            </Table>
          )}
        </Card>
        <Card title="Créer une société">
          <form
            onSubmit={handleSubmit((v) => create.mutate(v))}
            className="grid items-end gap-3 sm:grid-cols-3"
          >
            {create.isError && (
              <div className="sm:col-span-3">
                <ErrorBox error={create.error} />
              </div>
            )}
            <Field label="Nom">{(id) => <Input id={id} required {...register('name')} />}</Field>
            <Field label="Identifiant (a-z, 0-9, -)">
              {(id) => <Input id={id} required pattern="[a-z0-9-]{2,50}" {...register('slug')} />}
            </Field>
            <Field label="Devise">
              {(id) => <Input id={id} maxLength={3} required {...register('currency')} />}
            </Field>
            <Field label="Nom de l'admin RH">
              {(id) => <Input id={id} required {...register('admin_name')} />}
            </Field>
            <Field label="Email de l'admin RH">
              {(id) => <Input id={id} type="email" required {...register('admin_email')} />}
            </Field>
            <Field label="Mot de passe initial">
              {(id) => (
                <Input
                  id={id}
                  type="password"
                  autoComplete="new-password"
                  required
                  {...register('admin_password')}
                />
              )}
            </Field>
            <Button type="submit" loading={create.isPending}>
              Créer la société
            </Button>
          </form>
        </Card>
      </div>
    </>
  )
}
