import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import ThemeSwitch from './ThemeSwitch'

describe('ThemeSwitch', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.classList.remove('dark')
    window.matchMedia = ((q: string) => ({
      matches: false,
      media: q,
      addEventListener() {},
      removeEventListener() {},
    })) as unknown as typeof window.matchMedia
  })

  it('applique et mémorise sombre, puis clair', () => {
    render(<ThemeSwitch />)
    fireEvent.click(screen.getByRole('radio', { name: 'Sombre' }))
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(localStorage.getItem('mysuite-theme')).toBe('dark')
    fireEvent.click(screen.getByRole('radio', { name: 'Clair' }))
    expect(document.documentElement.classList.contains('dark')).toBe(false)
    expect(localStorage.getItem('mysuite-theme')).toBe('light')
  })

  it('« Système » efface le choix mémorisé', () => {
    localStorage.setItem('mysuite-theme', 'dark')
    render(<ThemeSwitch />)
    fireEvent.click(screen.getByRole('radio', { name: 'Système' }))
    expect(localStorage.getItem('mysuite-theme')).toBeNull()
    expect(screen.getByRole('radio', { name: 'Système' })).toHaveAttribute('aria-checked', 'true')
  })
})
