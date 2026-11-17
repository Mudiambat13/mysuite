// Affichage uniquement : les montants restent des chaînes décimales exactes côté API.
export function money(value: string | number | null | undefined, currency = 'USD'): string {
  if (value === null || value === undefined || value === '') return '—'
  try {
    return new Intl.NumberFormat('fr-FR', { style: 'currency', currency }).format(Number(value))
  } catch {
    return `${value} ${currency}`
  }
}

export function date(value: string | null | undefined): string {
  if (!value) return '—'
  const d = new Date(value.length === 10 ? `${value}T00:00:00` : value)
  return new Intl.DateTimeFormat('fr-FR', { dateStyle: 'medium' }).format(d)
}

export function dateTime(value: string): string {
  return new Intl.DateTimeFormat('fr-FR', { dateStyle: 'short', timeStyle: 'short' }).format(new Date(value))
}

const MONTHS = [
  'janvier',
  'février',
  'mars',
  'avril',
  'mai',
  'juin',
  'juillet',
  'août',
  'septembre',
  'octobre',
  'novembre',
  'décembre',
]
export function monthName(m: number): string {
  return MONTHS[m - 1] ?? String(m)
}
export function periodLabel(year: number, month: number): string {
  return `${monthName(month)} ${year}`
}

/** Minutes → « 8 h 05 » (ou « — » si inconnu). */
export function hoursMinutes(minutes: number | null | undefined): string {
  if (minutes === null || minutes === undefined) return '—'
  const h = Math.floor(minutes / 60)
  const m = Math.abs(minutes % 60)
  return `${h} h ${String(m).padStart(2, '0')}`
}

/** Heure d'un instant ISO affichée dans le fuseau de la société (pas celui du navigateur). */
export function timeInZone(iso: string, timeZone: string): string {
  return new Intl.DateTimeFormat('fr-FR', { timeStyle: 'short', timeZone }).format(new Date(iso))
}

export function dateInZone(iso: string, timeZone: string): string {
  return new Intl.DateTimeFormat('fr-FR', { dateStyle: 'medium', timeZone }).format(new Date(iso))
}

type Parts = { year: number; month: number; day: number; hour: number; minute: number; second: number }

function zoneParts(ms: number, timeZone: string): Parts {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone,
    hourCycle: 'h23',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).formatToParts(new Date(ms))
  const o: Parts = { year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0 }
  for (const p of parts) if (p.type in o) o[p.type as keyof Parts] = Number(p.value)
  return o
}

/** ISO UTC → « YYYY-MM-DDTHH:mm » (valeur d'un champ datetime-local) dans le fuseau donné. */
export function isoToZonedInput(iso: string, timeZone: string): string {
  const p = zoneParts(new Date(iso).getTime(), timeZone)
  const z = (n: number, l = 2) => String(n).padStart(l, '0')
  return `${z(p.year, 4)}-${z(p.month)}-${z(p.day)}T${z(p.hour)}:${z(p.minute)}`
}

/** « YYYY-MM-DDTHH:mm » lu dans le fuseau donné → ISO UTC. */
export function zonedInputToIso(local: string, timeZone: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(local)
  if (!m) throw new Error('date/heure invalide')
  const [y, mo, da, h, mi] = m.slice(1).map(Number) as [number, number, number, number, number]
  const asUtc = Date.UTC(y, mo - 1, da, h, mi)
  let guess = asUtc
  for (let i = 0; i < 2; i++) {
    const p = zoneParts(guess, timeZone)
    const shown = Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute)
    guess += asUtc - shown
  }
  return new Date(guess).toISOString()
}
