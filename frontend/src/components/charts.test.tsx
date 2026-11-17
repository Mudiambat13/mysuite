import { fireEvent, render, screen } from '@testing-library/react'
import { beforeAll, describe, expect, it, vi } from 'vitest'
import { InteractiveDonut, Segmented } from './charts'

describe('Segmented', () => {
  it('signale l’option active et notifie le changement', () => {
    const onChange = vi.fn()
    render(
      <Segmented
        label="Vue"
        value="a"
        onChange={onChange}
        options={[
          ['a', 'Anneau'],
          ['b', 'Barres'],
        ]}
      />,
    )
    expect(screen.getByRole('radio', { name: 'Anneau' })).toHaveAttribute('aria-checked', 'true')
    fireEvent.click(screen.getByRole('radio', { name: 'Barres' }))
    expect(onChange).toHaveBeenCalledWith('b')
  })
})

describe('InteractiveDonut', () => {
  beforeAll(() => {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  })

  it('affiche parts, pourcentages et total au centre, puis le détail au survol', () => {
    render(
      <InteractiveDonut
        label="x"
        data={[
          { label: 'RH', count: 3 },
          { label: 'Paie', count: 1 },
        ]}
      />,
    )
    expect(screen.getByText('Total')).toBeInTheDocument()
    expect(screen.getByText('3 · 75%')).toBeInTheDocument()
    fireEvent.mouseEnter(screen.getByRole('button', { name: /Paie/ }))
    expect(screen.getByText('Paie · 25%')).toBeInTheDocument()
  })
})
