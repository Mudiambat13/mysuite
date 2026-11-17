import { useCallback, useEffect, useState } from 'react'

export type ThemeMode = 'light' | 'dark' | 'system'
const KEY = 'mysuite-theme'

function stored(): ThemeMode {
  try {
    const v = localStorage.getItem(KEY)
    return v === 'light' || v === 'dark' ? v : 'system'
  } catch {
    return 'system'
  }
}

export function applyTheme(mode: ThemeMode) {
  const dark = mode === 'dark' || (mode === 'system' && matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.classList.toggle('dark', dark)
}

/** Thème clair / sombre / système, mémorisé par navigateur ; « système » suit le réglage de l'OS en direct. */
export function useTheme() {
  const [mode, setModeState] = useState<ThemeMode>(stored)

  const setMode = useCallback((m: ThemeMode) => {
    try {
      if (m === 'system') localStorage.removeItem(KEY)
      else localStorage.setItem(KEY, m)
    } catch {
      /* stockage indisponible : le thème vaut pour cette session */
    }
    setModeState(m)
    applyTheme(m)
  }, [])

  useEffect(() => {
    if (mode !== 'system') return
    const mq = matchMedia('(prefers-color-scheme: dark)')
    const on = () => applyTheme('system')
    mq.addEventListener('change', on)
    return () => mq.removeEventListener('change', on)
  }, [mode])

  return { mode, setMode }
}
