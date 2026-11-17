import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { ApiError } from '../lib/api'
import { EmptyState, ErrorBox, Pagination, StatusBadge } from './ui'

describe('composants', () => {
  it('affiche le libellé traduit du statut', () => {
    render(<StatusBadge status="pending" />)
    expect(screen.getByText('En attente')).toBeInTheDocument()
  })
  it("affiche l'état vide", () => {
    render(<EmptyState title="Aucun employé" hint="Créez-en un" />)
    expect(screen.getByText('Aucun employé')).toBeInTheDocument()
  })
  it('affiche une erreur API avec alerte accessible et référence', () => {
    render(<ErrorBox error={new ApiError(403, 'forbidden', 'accès refusé', 'abcdef1234')} />)
    expect(screen.getByRole('alert')).toHaveTextContent('accès refusé')
    expect(screen.getByRole('alert')).toHaveTextContent('abcdef12')
  })
  it('désactive Précédent sur la première page', () => {
    render(<Pagination page={1} limit={10} total={25} onChange={() => {}} />)
    expect(screen.getByText('Précédent')).toBeDisabled()
    expect(screen.getByText('Suivant')).toBeEnabled()
  })
})
