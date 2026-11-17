import { Download, Printer } from 'lucide-react'
import { money, periodLabel } from '../../lib/format'
import type { PayLine, PayslipDetail } from '../../lib/types'
import { Badge, Button, Card, Table, Td } from '../../components/ui'

const GROUPS: [string, string][] = [
  ['earning', 'Gains'],
  ['employee_contribution', 'Cotisations salariales'],
  ['tax', 'Impôts'],
  ['deduction', 'Retenues'],
  ['employer_contribution', 'Charges patronales (information)'],
]

export default function PayslipView({ slip }: { slip: PayslipDetail }) {
  const cur = slip.currency
  const r = slip.result
  return (
    <Card
      title={`Bulletin — ${slip.employee_name} · ${periodLabel(slip.year, slip.month)}`}
      actions={
        <span className="no-print flex items-center gap-2">
          {slip.simulation && <Badge tone="amber">Simulation</Badge>}
          <a
            href={`/api/v1/payslips/${slip.item_id}/pdf`}
            className="inline-flex items-center gap-2 rounded-lg border border-slate-300 px-3.5 py-2 text-sm font-medium hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800"
          >
            <Download className="h-4 w-4" aria-hidden /> PDF
          </a>
          <Button variant="secondary" onClick={() => window.print()}>
            <Printer className="h-4 w-4" aria-hidden /> Imprimer
          </Button>
        </span>
      }
    >
      <p className="mb-4 text-xs text-slate-500">
        Matricule {slip.matricule} · Règles « {slip.rule_set_name} » v{slip.rule_version}
      </p>
      {GROUPS.map(([kind, label]) => {
        const lines = r.lines.filter((l: PayLine) => l.kind === kind)
        if (lines.length === 0) return null
        return (
          <div key={kind} className="mb-5">
            <h3 className="mb-1 text-sm font-semibold">{label}</h3>
            <Table head={['Libellé', 'Base', 'Taux', 'Montant']}>
              {lines.map((l) => (
                <tr key={l.code + l.label}>
                  <Td>
                    {l.label}
                    {l.note && <span className="block text-xs text-slate-500">{l.note}</span>}
                  </Td>
                  <Td className="tabular-nums">{l.base ? money(l.base, cur) : '—'}</Td>
                  <Td className="tabular-nums">{l.rate ? `${(Number(l.rate) * 100).toString()} %` : '—'}</Td>
                  <Td className="tabular-nums">{money(l.amount, cur)}</Td>
                </tr>
              ))}
            </Table>
          </div>
        )
      })}
      <dl className="mt-4 grid gap-3 border-t border-slate-200 pt-4 sm:grid-cols-3 dark:border-slate-700">
        <div>
          <dt className="text-xs uppercase text-slate-500">Brut</dt>
          <dd className="text-lg font-semibold tabular-nums">{money(r.gross, cur)}</dd>
        </div>
        <div>
          <dt className="text-xs uppercase text-slate-500">Retenues totales</dt>
          <dd className="text-lg font-semibold tabular-nums">
            {money(
              String(Number(r.total_employee_contributions) + Number(r.tax) + Number(r.total_deductions)),
              cur,
            )}
          </dd>
        </div>
        <div>
          <dt className="text-xs uppercase text-slate-500">Net à payer</dt>
          <dd className="text-lg font-semibold tabular-nums text-brand-700 dark:text-brand-100">
            {money(r.net, cur)}
          </dd>
        </div>
      </dl>
    </Card>
  )
}
