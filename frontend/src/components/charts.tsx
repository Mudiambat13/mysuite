import { useState } from 'react'
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { cx } from './ui'

export const PALETTE = [
  '#3b6cf6',
  '#e0803a',
  '#2f9e75',
  '#8a6bd6',
  '#d6477a',
  '#2aa7c9',
  '#b8a21f',
  '#64748b',
]

export interface Series {
  key: string
  name: string
  color?: string
}
type Kind = 'bar' | 'line' | 'area'
type Datum = object

/** Bouton segmenté accessible (radiogroup). */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: [T, string][]
  onChange: (v: T) => void
  label: string
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-lg border border-slate-300 p-0.5 text-xs dark:border-slate-700"
    >
      {options.map(([k, text]) => (
        <button
          key={k}
          role="radio"
          aria-checked={value === k}
          onClick={() => onChange(k)}
          className={cx(
            'rounded-md px-2.5 py-1 font-medium',
            value === k
              ? 'bg-brand-600 text-white'
              : 'text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800',
          )}
        >
          {text}
        </button>
      ))}
    </div>
  )
}

/**
 * Graphique interactif : survol (infobulle), clic sur la légende pour masquer/afficher une série,
 * bascule barres / courbe / aires, clic sur un point pour afficher son détail.
 */
export function InteractiveChart({
  data,
  xKey,
  series,
  label,
  format = (n) => String(n),
  height = 256,
  kinds = ['bar', 'line', 'area'],
}: {
  data: Datum[]
  xKey: string
  series: Series[]
  label: string
  format?: (n: number) => string
  height?: number
  kinds?: Kind[]
}) {
  const [kind, setKind] = useState<Kind>(kinds[0] ?? 'bar')
  const [hidden, setHidden] = useState<string[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const visible = series.filter((s) => !hidden.includes(s.key))
  const color = (s: Series, i: number) => s.color ?? PALETTE[i % PALETTE.length]
  const toggle = (key: string) =>
    setHidden((h) =>
      h.includes(key) ? h.filter((k) => k !== key) : series.length - h.length > 1 ? [...h, key] : h,
    )
  const picked = data.find((d) => String((d as Record<string, unknown>)[xKey]) === selected)
  const onPoint = (d: unknown) => {
    const p = (d as { activeLabel?: string | number } | null)?.activeLabel
    if (p !== undefined) setSelected((cur) => (cur === String(p) ? null : String(p)))
  }
  const common = {
    data,
    margin: { left: 0, right: 8 },
    onClick: onPoint,
  }
  const axes = [
    <CartesianGrid key="grid" strokeDasharray="3 3" vertical={false} stroke="currentColor" opacity={0.15} />,
    <XAxis key="x" dataKey={xKey} tick={{ fontSize: 12 }} />,
    <YAxis
      key="y"
      allowDecimals={false}
      tick={{ fontSize: 12 }}
      width={56}
      tickFormatter={(v: number) => format(v)}
    />,
    <Tooltip
      key="tip"
      formatter={(v) => format(Number(v))}
      cursor={{ fill: 'currentColor', opacity: 0.06 }}
    />,
  ]
  return (
    <div>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <p className="text-xs text-slate-500">
          Cliquez la légende pour masquer une série, un point pour son détail.
        </p>
        {kinds.length > 1 && (
          <Segmented
            label="Type de graphique"
            value={kind}
            onChange={setKind}
            options={kinds.map(
              (k) => [k, { bar: 'Barres', line: 'Courbe', area: 'Aires' }[k]] as [Kind, string],
            )}
          />
        )}
      </div>
      <div style={{ height }} role="img" aria-label={label}>
        <ResponsiveContainer>
          {kind === 'bar' ? (
            <BarChart {...common}>
              {axes}
              {visible.map((s) => (
                <Bar
                  key={s.key}
                  dataKey={s.key}
                  name={s.name}
                  fill={color(s, series.indexOf(s))}
                  radius={[4, 4, 0, 0]}
                  cursor="pointer"
                >
                  {data.map((d) => (
                    <Cell
                      key={String((d as Record<string, unknown>)[xKey])}
                      fillOpacity={
                        selected && String((d as Record<string, unknown>)[xKey]) !== selected ? 0.35 : 1
                      }
                    />
                  ))}
                </Bar>
              ))}
            </BarChart>
          ) : kind === 'line' ? (
            <LineChart {...common}>
              {axes}
              {visible.map((s) => (
                <Line
                  key={s.key}
                  type="monotone"
                  dataKey={s.key}
                  name={s.name}
                  stroke={color(s, series.indexOf(s))}
                  strokeWidth={2}
                  dot={{ r: 3 }}
                  activeDot={{ r: 6 }}
                />
              ))}
            </LineChart>
          ) : (
            <AreaChart {...common}>
              {axes}
              {visible.map((s) => (
                <Area
                  key={s.key}
                  type="monotone"
                  dataKey={s.key}
                  name={s.name}
                  stroke={color(s, series.indexOf(s))}
                  fill={color(s, series.indexOf(s))}
                  fillOpacity={0.25}
                />
              ))}
            </AreaChart>
          )}
        </ResponsiveContainer>
      </div>
      <div className="mt-2 flex flex-wrap justify-center gap-2">
        {series.map((s, i) => {
          const off = hidden.includes(s.key)
          return (
            <button
              key={s.key}
              aria-pressed={!off}
              onClick={() => toggle(s.key)}
              className={cx(
                'flex items-center gap-2 rounded-full border border-slate-200 px-3 py-1 text-xs dark:border-slate-700',
                off && 'opacity-50',
              )}
            >
              <span className="h-2.5 w-2.5 rounded-full" style={{ background: color(s, i) }} aria-hidden />
              <span className={off ? 'line-through' : ''}>{s.name}</span>
            </button>
          )
        })}
      </div>
      {picked && (
        <div
          className="mt-3 rounded-lg border border-brand-100 bg-brand-50 p-3 text-sm dark:border-slate-700 dark:bg-slate-800"
          role="status"
        >
          <div className="flex items-center justify-between">
            <strong>{String((picked as Record<string, unknown>)[xKey])}</strong>
            <button onClick={() => setSelected(null)} className="text-xs text-slate-500 hover:underline">
              Fermer
            </button>
          </div>
          <ul className="mt-1 flex flex-wrap gap-x-6 gap-y-1">
            {series.map((s, i) => (
              <li key={s.key} className="flex items-center gap-2">
                <span className="h-2.5 w-2.5 rounded-full" style={{ background: color(s, i) }} aria-hidden />
                {s.name} :{' '}
                <span className="font-medium tabular-nums">
                  {format(Number((picked as Record<string, unknown>)[s.key]))}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  )
}

/** Anneau de répartition interactif : survol/clic sur une part, pourcentage, légende cliquable. */
export function InteractiveDonut({
  data,
  label,
  total,
}: {
  data: { label: string; count: number }[]
  label: string
  total?: number
}) {
  const [active, setActive] = useState<number | null>(null)
  const sum = total ?? data.reduce((n, d) => n + d.count, 0)
  const cur = active !== null ? data[active] : null
  return (
    <div className="grid items-center gap-4 sm:grid-cols-2">
      <div className="relative h-56" role="img" aria-label={label}>
        <ResponsiveContainer>
          <PieChart>
            <Pie
              data={data}
              dataKey="count"
              nameKey="label"
              innerRadius="62%"
              outerRadius="92%"
              paddingAngle={2}
              onMouseEnter={(_, i) => setActive(i)}
              onMouseLeave={() => setActive(null)}
              onClick={(_, i) => setActive((a) => (a === i ? null : i))}
            >
              {data.map((d, i) => (
                <Cell
                  key={d.label}
                  fill={PALETTE[i % PALETTE.length]}
                  fillOpacity={active === null || active === i ? 1 : 0.35}
                  cursor="pointer"
                />
              ))}
            </Pie>
          </PieChart>
        </ResponsiveContainer>
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center text-center">
          <span className="text-2xl font-semibold tabular-nums">{cur ? cur.count : sum}</span>
          <span className="max-w-[7rem] truncate text-xs text-slate-500">
            {cur ? `${cur.label} · ${sum ? Math.round((cur.count / sum) * 100) : 0}%` : 'Total'}
          </span>
        </div>
      </div>
      <ul className="space-y-1 text-sm">
        {data.map((d, i) => (
          <li key={d.label}>
            <button
              onMouseEnter={() => setActive(i)}
              onMouseLeave={() => setActive(null)}
              onFocus={() => setActive(i)}
              onBlur={() => setActive(null)}
              className="flex w-full items-center gap-2 rounded px-2 py-1 text-left hover:bg-slate-100 dark:hover:bg-slate-800"
            >
              <span
                className="h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ background: PALETTE[i % PALETTE.length] }}
                aria-hidden
              />
              <span className="flex-1 truncate">{d.label}</span>
              <span className="tabular-nums text-slate-500">
                {d.count} · {sum ? Math.round((d.count / sum) * 100) : 0}%
              </span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
