import { createContext, useContext, type ReactNode } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { api, ApiError } from '../lib/api'
import type { Me } from '../lib/types'

interface AuthCtx {
  me: Me | null
  loading: boolean
  refresh: () => Promise<void>
  can: (permission: string) => boolean
}

const Ctx = createContext<AuthCtx | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const qc = useQueryClient()
  const q = useQuery({
    queryKey: ['me'],
    queryFn: async () => {
      try {
        return await api.get<Me>('/auth/me')
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) return null
        throw e
      }
    },
    retry: false,
    staleTime: 60_000,
  })
  const me = q.data ?? null
  const value: AuthCtx = {
    me,
    loading: q.isLoading,
    refresh: async () => {
      await qc.invalidateQueries({ queryKey: ['me'] })
    },
    // L'affichage suit les permissions, mais la sécurité réelle est appliquée côté serveur.
    can: (p) => !!me?.permissions.includes(p),
  }
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>
}

export function useAuth(): AuthCtx {
  const v = useContext(Ctx)
  if (!v) throw new Error('useAuth hors AuthProvider')
  return v
}
