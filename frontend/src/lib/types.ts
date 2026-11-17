export interface TenantInfo {
  id: string
  name: string
  slug: string
  role: string
  currency: string
}
export interface Me {
  user: { id: string; email: string; full_name: string; is_platform_admin: boolean }
  tenants: TenantInfo[]
  active_tenant_id: string | null
  role: string | null
  permissions: string[]
  employee_id: string | null
  mfa_enrollment_required?: boolean
}
export interface Page<T> {
  items: T[]
  total: number
  page: number
  limit: number
}
export interface Employee {
  id: string
  matricule: string
  first_name: string
  last_name: string
  email: string | null
  phone: string | null
  birth_date: string | null
  hire_date: string
  exit_date: string | null
  status: string
  contract_type: string
  site_id: string | null
  site_name: string | null
  department_id: string | null
  department_name: string | null
  position_id: string | null
  position_title: string | null
  manager_id: string | null
}
export interface Site {
  id: string
  name: string
  city: string | null
}
export interface Department {
  id: string
  name: string
  parent_id: string | null
}
export interface Position {
  id: string
  title: string
  level: string | null
}
export interface LeaveType {
  id: string
  code: string
  name: string
  days_per_year: string
  requires_balance: boolean
  active: boolean
}
export interface LeaveBalance {
  leave_type_id: string
  leave_type_name: string
  year: number
  entitled: string
  used: string
  pending: string
}
export interface LeaveRequest {
  id: string
  employee_id: string
  employee_name: string
  leave_type_id: string
  leave_type_name: string
  start_date: string
  end_date: string
  days: string
  reason: string | null
  status: string
  decision_comment: string | null
}
export interface Compensation {
  id: string
  base_salary: string
  currency: string
  effective_from: string
}
export interface PayItem {
  id: string
  kind: string
  code: string
  label: string
  amount: string
  active_from: string
  active_to: string | null
}
export interface RuleSet {
  id: string
  country: string
  name: string
  version: number
  effective_from: string
  status: string
  is_demo: boolean
  source: string | null
  verified_on: string | null
  created_by: string | null
  params: unknown
}
export interface Period {
  id: string
  year: number
  month: number
  status: string
}
export interface Run {
  id: string
  period_id: string
  year: number
  month: number
  rule_set_id: string
  rule_version: number
  rule_set_name: string
  simulation: boolean
  status: string
  total_gross: string
  total_net: string
  total_employer_cost: string
  employee_count: number
}
export interface RunItem {
  id: string
  employee_id: string
  matricule: string
  employee_name: string
  gross: string
  net: string
  employer_cost: string
}
export interface RunDetail {
  run: Run
  items: RunItem[]
  warnings: string[]
}
export interface PayLine {
  kind: string
  code: string
  label: string
  base: string | null
  rate: string | null
  amount: string
  note?: string | null
}
export interface PayslipSummary {
  item_id: string
  year: number
  month: number
  gross: string
  net: string
}
export interface PayslipDetail {
  item_id: string
  employee_id: string
  matricule: string
  employee_name: string
  year: number
  month: number
  run_status: string
  simulation: boolean
  rule_set_name: string
  rule_version: number
  currency: string
  result: {
    gross: string
    net: string
    employer_cost: string
    tax: string
    total_employee_contributions: string
    total_employer_contributions: string
    total_deductions: string
    lines: PayLine[]
  }
}
export interface Bucket {
  label: string
  count: number
}
export interface Headcount {
  total_active: number
  by_status: Bucket[]
  by_department: Bucket[]
  by_site: Bucket[]
  by_contract: Bucket[]
}
export interface Dashboard {
  active_employees: number | null
  pending_leave_to_review: number | null
  my_pending_leave: number
  on_leave_today: number | null
}
export interface UserRow {
  user_id: string
  email: string
  full_name: string
  role: string
  status: string
}
export interface AuditRow {
  id: number
  actor_name: string | null
  action: string
  entity: string
  entity_id: string | null
  details: unknown
  created_at: string
}
export interface TenantRow {
  id: string
  name: string
  slug: string
  country: string
  currency: string
  status: string
}
export interface SessionInfo {
  id: string
  user_agent: string | null
  ip: string | null
  created_at: string
  last_seen_at: string
  current: boolean
}
export interface Contract {
  id: string
  employee_id: string
  employee_name: string
  matricule: string
  contract_type: string
  start_date: string
  end_date: string | null
  probation_end: string | null
  job_title: string | null
  status: string
  notes: string | null
}
export interface Amendment {
  id: string
  effective_date: string
  description: string
  new_end_date: string | null
}
export interface MovementRow {
  employee_id: string
  matricule: string
  name: string
  date: string
  department_name: string | null
}
export interface MonthRow {
  month: string
  entries: number
  exits: number
}
export interface Movements {
  from: string
  to: string
  entries: MovementRow[]
  exits: MovementRow[]
  by_month: MonthRow[]
}
export interface AbsenceRow {
  label: string
  days: string
  requests: number
}
export interface Absences {
  from: string
  to: string
  total_days: string
  by_type: AbsenceRow[]
  by_department: AbsenceRow[]
}
export interface CostMonth {
  month: number
  gross: string
  net: string
  employer_cost: string
  employees: number
}
export interface CostDept {
  label: string
  gross: string
  employer_cost: string
  employees: number
}
export interface PayrollCost {
  year: number
  currency: string
  months: CostMonth[]
  by_department: CostDept[]
}
export interface AttendanceSettings {
  timezone: string
  work_start: string
  grace_minutes: number
  daily_minutes: number
}
export interface AttendanceEntry {
  id: string
  employee_id: string
  employee_name: string
  matricule: string
  clock_in: string
  clock_out: string | null
  minutes: number | null
  source: 'self' | 'manual' | 'device' | 'import'
  note: string | null
  corrected: boolean
}
export interface MyAttendance {
  settings: AttendanceSettings
  open: AttendanceEntry | null
  today: AttendanceEntry[]
  today_minutes: number
}
export interface AttendanceSummaryRow {
  employee_id: string
  matricule: string
  name: string
  days_present: number
  worked_minutes: number
  late_days: number
  overtime_minutes: number
  open_days: number
}
export interface AttendanceSummary {
  from: string
  to: string
  settings: AttendanceSettings
  rows: AttendanceSummaryRow[]
}
export interface Device {
  id: string
  name: string
  key_prefix: string
  active: boolean
  last_seen_at: string | null
  created_at: string
}
export interface CreatedDevice {
  id: string
  name: string
  api_key: string
}
export interface Badge {
  id: string
  employee_id: string
  employee_name: string
  matricule: string
  badge_code: string
}
export interface PunchOutcome {
  line: number | null
  identifier: string
  at: string | null
  status: string
  reason: string | null
  employee_name: string | null
}
export interface IngestResult {
  dry_run: boolean
  counts: {
    total: number
    entries_opened: number
    entries_closed: number
    duplicates: number
    unknown_badges: number
    rejected: number
  }
  rows: PunchOutcome[]
}
export interface RawPunch {
  id: string
  source: string
  device_name: string | null
  identifier: string
  punched_at: string
  direction: string | null
  employee_name: string | null
  status: string
  reason: string | null
}

export interface MfaStatus {
  available: boolean
  enabled: boolean
  recovery_codes_remaining: number
}
export interface MfaSetup {
  secret: string
  otpauth_uri: string
}
export interface RecoveryCodes {
  recovery_codes: string[]
}
export interface PasswordResetIssued {
  reset_token: string
  path: string
  expires_at: string
  emailed: boolean
}
export type LoginResult = Me | { mfa_required: true; mfa_token: string }
export function needsMfa(r: LoginResult): r is { mfa_required: true; mfa_token: string } {
  return 'mfa_required' in r && r.mfa_required === true
}

export interface Dependents {
  id: string
  dependents: number
  effective_from: string
}
export interface EmployeeDocument {
  id: string
  employee_id: string
  kind: 'identity' | 'diploma' | 'certificate' | 'other'
  name: string
  content_type: string
  size_bytes: number
  expires_on: string | null
  uploaded_at: string
}
export interface ExpiringDocument {
  id: string
  employee_id: string
  employee_name: string
  matricule: string
  kind: string
  name: string
  expires_on: string
}

export interface ContractTemplate {
  id: string
  name: string
  body: string
  created_at: string
}

export interface WageGrid {
  id: string
  name: string
  currency: string
  effective_from: string
  source: string
  row_count: number
}
export interface WageGridRow {
  class_no: number
  category: string
  reference: string
  coefficient: string | null
  daily: string
  monthly: string
}
export interface WageClass {
  id: string
  class_no: number
  effective_from: string
}
export type SmigStatus = 'ok' | 'below' | 'no_class' | 'no_salary' | 'no_grid' | 'currency_mismatch'
export interface SmigRow {
  employee_id: string
  matricule: string
  full_name: string
  class_no: number | null
  base_salary: string | null
  salary_currency: string | null
  minimum_monthly: string | null
  status: SmigStatus
  gap: string | null
}
export interface SmigReport {
  as_of: string
  below_minimum: number
  rows: SmigRow[]
}

export interface RuleTemplate {
  variante?: string
  id: string
  country: string
  name: string
  currency: string
  period: string
  effective_from: string
  avertissement: string
  params: unknown
  provenance: { champ: string; valeur: string; confiance: string; sources: string[] }[]
  a_completer: string[]
  sources: Record<string, { titre: string; url: string; type: string }>
  consulte_le: string
}
