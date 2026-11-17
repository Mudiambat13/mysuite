import { Monitor, Moon, Sun } from 'lucide-react'
import { useTheme, type ThemeMode } from '../hooks/theme'
import { cx } from './ui'

const OPTIONS: [ThemeMode, string, typeof Sun][] = [
  ['light', 'Clair', Sun],
  ['system', 'Système', Monitor],
  ['dark', 'Sombre', Moon],
]

export default function ThemeSwitch({ className }: { className?: string }) {
  const { mode, setMode } = useTheme()
  return (
    <div
      role="radiogroup"
      aria-label="Thème"
      className={cx('inline-flex rounded-lg border border-slate-300 p-0.5 dark:border-slate-700', className)}
    >
      {OPTIONS.map(([k, label, Icon]) => (
        <button
          key={k}
          role="radio"
          aria-checked={mode === k}
          aria-label={label}
          title={label}
          onClick={() => setMode(k)}
          className={cx(
            'rounded-md p-1.5',
            mode === k
              ? 'bg-brand-600 text-white'
              : 'text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800',
          )}
        >
          <Icon className="h-4 w-4" aria-hidden />
        </button>
      ))}
    </div>
  )
}
