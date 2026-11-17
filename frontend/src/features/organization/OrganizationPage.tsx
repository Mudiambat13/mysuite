import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '../../lib/api'
import { useAuth } from '../../app/auth'
import {
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

interface Row {
  id: string
  label: string
  extra: string
}

function Section({
  title,
  path,
  queryKey,
  fieldLabel,
  extraLabel,
  toRow,
  buildBody,
}: {
  title: string
  path: string
  queryKey: string
  fieldLabel: string
  extraLabel: string
  toRow: (r: never) => Row
  buildBody: (name: string, extra: string) => unknown
}) {
  const { me, can } = useAuth()
  const qc = useQueryClient()
  const key = [queryKey, me?.active_tenant_id]
  const list = useQuery({ queryKey: key, queryFn: () => api.get<never[]>(path) })
  const { register, handleSubmit, reset } = useForm<{ name: string; extra: string }>()
  const add = useMutation({
    mutationFn: (v: { name: string; extra: string }) => api.post(path, buildBody(v.name, v.extra)),
    onSuccess: async () => {
      reset()
      await qc.invalidateQueries({ queryKey: [queryKey] })
    },
  })
  return (
    <Card title={title}>
      {list.isLoading ? (
        <Skeleton rows={2} />
      ) : list.isError ? (
        <ErrorBox error={list.error} />
      ) : list.data!.length === 0 ? (
        <EmptyState title="Aucun élément" />
      ) : (
        <Table head={[fieldLabel, extraLabel]}>
          {list.data!.map((r) => {
            const row = toRow(r)
            return (
              <tr key={row.id}>
                <Td className="font-medium">{row.label}</Td>
                <Td>{row.extra || '—'}</Td>
              </tr>
            )
          })}
        </Table>
      )}
      {can('org_manage') && (
        <form
          onSubmit={handleSubmit((v) => add.mutate(v))}
          className="mt-4 grid items-end gap-3 sm:grid-cols-3"
        >
          {add.isError && (
            <div className="sm:col-span-3">
              <ErrorBox error={add.error} />
            </div>
          )}
          <Field label={fieldLabel}>{(id) => <Input id={id} required {...register('name')} />}</Field>
          <Field label={extraLabel}>{(id) => <Input id={id} {...register('extra')} />}</Field>
          <Button type="submit" loading={add.isPending}>
            Ajouter
          </Button>
        </form>
      )}
    </Card>
  )
}

export default function OrganizationPage() {
  return (
    <>
      <PageHeader title="Organisation" subtitle="Sites, départements et postes de la société" />
      <div className="grid gap-6 lg:grid-cols-2">
        <Section
          title="Sites"
          path="/org/sites"
          queryKey="sites"
          fieldLabel="Nom"
          extraLabel="Ville"
          toRow={(r: { id: string; name: string; city: string | null }) =>
            ({ id: r.id, label: r.name, extra: r.city ?? '' }) as never
          }
          buildBody={(name, extra) => ({ name, city: extra || null })}
        />
        <Section
          title="Départements"
          path="/org/departments"
          queryKey="departments"
          fieldLabel="Nom"
          extraLabel="—"
          toRow={(r: { id: string; name: string }) => ({ id: r.id, label: r.name, extra: '' }) as never}
          buildBody={(name) => ({ name })}
        />
        <Section
          title="Postes"
          path="/org/positions"
          queryKey="positions"
          fieldLabel="Intitulé"
          extraLabel="Niveau"
          toRow={(r: { id: string; title: string; level: string | null }) =>
            ({ id: r.id, label: r.title, extra: r.level ?? '' }) as never
          }
          buildBody={(title, extra) => ({ title, level: extra || null })}
        />
      </div>
    </>
  )
}
