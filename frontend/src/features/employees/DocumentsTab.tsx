import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, uploadFile } from '../../lib/api'
import { useAuth } from '../../app/auth'
import { date } from '../../lib/format'
import type { EmployeeDocument } from '../../lib/types'
import { Badge, Button, Card, EmptyState, ErrorBox, Field, Input, Select, Skeleton, Table, Td } from '../../components/ui'

export const KIND_LABEL: Record<string, string> = {
  identity: "Pièce d'identité",
  diploma: 'Diplôme',
  certificate: 'Certificat',
  other: 'Autre',
}

export function expiryBadge(expires: string | null) {
  if (!expires) return null
  const n = Math.ceil((new Date(`${expires}T00:00:00`).getTime() - Date.now()) / 86_400_000)
  if (n < 0) return <Badge tone="red">expirée</Badge>
  if (n <= 60) return <Badge tone="amber">dans {n} j</Badge>
  return <Badge tone="slate">valide</Badge>
}

const size = (b: number) => (b >= 1048576 ? `${(b / 1048576).toFixed(1)} Mo` : `${Math.max(1, Math.round(b / 1024))} Ko`)

export default function DocumentsTab({ id }: { id: string }) {
  const { can } = useAuth()
  const qc = useQueryClient()
  const manage = can('documents_manage')
  const list = useQuery({
    queryKey: ['documents', id],
    queryFn: () => api.get<EmployeeDocument[]>(`/employees/${id}/documents`),
  })
  const { register, handleSubmit, reset } = useForm<{ name: string; kind: string; expires_on: string }>({
    defaultValues: { kind: 'identity' },
  })
  const [file, setFile] = useState<File | null>(null)
  const refresh = async () => {
    await qc.invalidateQueries({ queryKey: ['documents', id] })
    await qc.invalidateQueries({ queryKey: ['documents-expiring'] })
  }
  const upload = useMutation({
    mutationFn: (v: { name: string; kind: string; expires_on: string }) => {
      const p = new URLSearchParams({ name: v.name, kind: v.kind })
      if (v.expires_on) p.set('expires_on', v.expires_on)
      return uploadFile(`/employees/${id}/documents?${p}`, file!)
    },
    onSuccess: async () => {
      reset({ kind: 'identity', name: '', expires_on: '' })
      setFile(null)
      await refresh()
    },
  })
  const del = useMutation({
    mutationFn: (docId: string) => api.del(`/documents/${docId}`),
    onSuccess: refresh,
  })
  return (
    <div className="space-y-6">
      <Card title="Pièces du dossier">
        {del.isError && <ErrorBox error={del.error} />}
        {list.isLoading ? (
          <Skeleton rows={2} />
        ) : list.isError ? (
          <ErrorBox error={list.error} />
        ) : list.data!.length === 0 ? (
          <EmptyState title="Aucune pièce" hint="PDF, PNG ou JPEG, 5 Mo maximum." />
        ) : (
          <Table head={['Nom', 'Type', 'Taille', 'Expiration', 'Déposée le', '']}>
            {list.data!.map((d) => (
              <tr key={d.id}>
                <Td className="font-medium">{d.name}</Td>
                <Td>{KIND_LABEL[d.kind]}</Td>
                <Td className="tabular-nums">{size(d.size_bytes)}</Td>
                <Td>
                  {d.expires_on ? date(d.expires_on) : '—'} {expiryBadge(d.expires_on)}
                </Td>
                <Td>{date(d.uploaded_at)}</Td>
                <Td className="text-right">
                  <span className="inline-flex gap-2">
                    <a
                      className="rounded-lg px-3 py-1.5 text-sm font-medium text-brand-600 hover:underline"
                      href={`/api/v1/documents/${d.id}/download`}
                    >
                      Télécharger
                    </a>
                    {manage && (
                      <Button
                        variant="secondary"
                        onClick={() => {
                          if (confirm(`Supprimer « ${d.name} » ? Le fichier sera effacé définitivement.`))
                            del.mutate(d.id)
                        }}
                      >
                        Supprimer
                      </Button>
                    )}
                  </span>
                </Td>
              </tr>
            ))}
          </Table>
        )}
      </Card>
      {manage && (
        <Card title="Déposer une pièce">
          <form
            onSubmit={handleSubmit((v) => file && upload.mutate(v))}
            className="grid items-end gap-3 sm:grid-cols-2 lg:grid-cols-4"
          >
            {upload.isError && (
              <div className="sm:col-span-2 lg:col-span-4">
                <ErrorBox error={upload.error} />
              </div>
            )}
            <Field label="Nom de la pièce">{(i) => <Input id={i} required {...register('name')} />}</Field>
            <Field label="Type">
              {(i) => (
                <Select id={i} {...register('kind')}>
                  {Object.entries(KIND_LABEL).map(([k, l]) => (
                    <option key={k} value={k}>
                      {l}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label="Expire le (facultatif)">
              {(i) => <Input id={i} type="date" {...register('expires_on')} />}
            </Field>
            <Field label="Fichier">
              {(i) => (
                <input
                  id={i}
                  type="file"
                  required
                  accept="application/pdf,image/png,image/jpeg"
                  className="text-sm"
                  onChange={(e) => setFile(e.target.files?.[0] ?? null)}
                />
              )}
            </Field>
            <div className="sm:col-span-2 lg:col-span-4">
              <Button type="submit" loading={upload.isPending}>
                Déposer
              </Button>
            </div>
          </form>
        </Card>
      )}
    </div>
  )
}
