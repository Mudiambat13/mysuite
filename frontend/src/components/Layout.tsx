import { useEffect, useState } from 'react'
import { useLocation } from 'react-router-dom'
import { NavLink, Outlet, useNavigate } from 'react-router-dom'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  Building2,
  CalendarDays,
  ClipboardList,
  FileSignature,
  FileText,
  BarChart3,
  Clock,
  LayoutDashboard,
  ShieldCheck,
  LogOut,
  Menu,
  X,
  ScrollText,
  Users,
  UserCog,
  Wallet,
} from 'lucide-react'
import { useAuth } from '../app/auth'
import { api } from '../lib/api'
import ThemeSwitch from './ThemeSwitch'
import { cx } from './ui'

interface NavItem {
  to: string
  labelKey: string
  icon: typeof Users
  show: boolean
}

export default function Layout() {
  const { me, can, refresh } = useAuth()
  const { t } = useTranslation()
  const nav = useNavigate()
  const qc = useQueryClient()
  const [collapsed, setCollapsed] = useState(false)
  const [open, setOpen] = useState(false)
  const location = useLocation()
  useEffect(() => setOpen(false), [location.pathname])
  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open])

  const logout = useMutation({
    mutationFn: () => api.post('/auth/logout'),
    onSuccess: async () => {
      qc.clear()
      await refresh()
      nav('/login')
    },
  })
  const switchTenant = useMutation({
    mutationFn: (tenant_id: string) => api.post('/auth/switch-tenant', { tenant_id }),
    onSuccess: async () => {
      await qc.invalidateQueries()
      nav('/')
    },
  })

  if (!me) return null
  const hasTenant = !!me.active_tenant_id
  const items: NavItem[] = [
    { to: '/', labelKey: 'nav.dashboard', icon: LayoutDashboard, show: hasTenant },
    { to: '/employees', labelKey: 'nav.employees', icon: Users, show: can('employees_read') },
    { to: '/contracts', labelKey: 'nav.contracts', icon: FileSignature, show: can('contracts_read') },
    { to: '/organization', labelKey: 'nav.organization', icon: Building2, show: can('org_read') },
    { to: '/leave', labelKey: 'nav.leave', icon: CalendarDays, show: can('leave_request_own') },
    { to: '/payroll', labelKey: 'nav.payroll', icon: Wallet, show: can('payroll_read') },
    {
      to: '/payslips',
      labelKey: 'nav.payslips',
      icon: FileText,
      show: can('payslip_read_own') && !!me.employee_id,
    },
    { to: '/attendance', labelKey: 'nav.attendance', icon: Clock, show: can('attendance_clock_own') },
    { to: '/reports', labelKey: 'nav.reports', icon: BarChart3, show: can('reports_read') },
    { to: '/security', labelKey: 'nav.security', icon: ShieldCheck, show: true },
    { to: '/users', labelKey: 'nav.users', icon: UserCog, show: can('users_manage') },
    { to: '/audit', labelKey: 'nav.audit', icon: ScrollText, show: can('audit_read') },
    { to: '/platform', labelKey: 'nav.platform', icon: ClipboardList, show: me.user.is_platform_admin },
  ]
  const active = me.tenants.find((x) => x.id === me.active_tenant_id)

  return (
    <div className="flex min-h-screen">
      {open && (
        <div
          className="no-print fixed inset-0 z-30 bg-slate-900/50 md:hidden"
          onClick={() => setOpen(false)}
          aria-hidden
        />
      )}
      <aside
        className={cx(
          'no-print fixed inset-y-0 left-0 z-40 flex h-screen w-64 shrink-0 flex-col border-r border-slate-200 bg-white transition-transform dark:border-slate-800 dark:bg-slate-900 md:sticky md:top-0 md:z-auto md:translate-x-0 md:transition-all',
          open ? 'translate-x-0' : '-translate-x-full',
          collapsed ? 'md:w-16' : 'md:w-60',
        )}
      >
        <div className="flex h-14 items-center gap-2 border-b border-slate-100 px-4 dark:border-slate-800">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-brand-600 font-bold text-white">
            M
          </div>
          <span className={cx('text-lg font-semibold tracking-tight', collapsed && 'md:hidden')}>
            MySuite
          </span>
          <button
            onClick={() => setOpen(false)}
            className="ml-auto rounded-lg p-2 text-slate-500 hover:bg-slate-100 md:hidden dark:hover:bg-slate-800"
            aria-label="Fermer le menu"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <nav className="flex-1 space-y-1 overflow-y-auto p-2" aria-label="Navigation principale">
          {items
            .filter((i) => i.show)
            .map((i) => (
              <NavLink
                key={i.to}
                to={i.to}
                end={i.to === '/'}
                title={t(i.labelKey)}
                className={({ isActive }) =>
                  cx(
                    'flex items-center gap-3 rounded-lg px-3 py-2 text-sm font-medium',
                    isActive
                      ? 'bg-brand-50 text-brand-700 dark:bg-slate-800 dark:text-brand-100'
                      : 'text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800',
                  )
                }
              >
                <i.icon className="h-4 w-4 shrink-0" aria-hidden />
                <span className={cx(collapsed && 'md:hidden')}>{t(i.labelKey)}</span>
              </NavLink>
            ))}
        </nav>
        <button
          onClick={() => setCollapsed(!collapsed)}
          className="m-2 hidden items-center gap-3 md:flex rounded-lg px-3 py-2 text-sm text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800"
          aria-label="Réduire ou étendre le menu"
        >
          <Menu className="h-4 w-4" aria-hidden />
          {!collapsed && 'Réduire'}
        </button>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="no-print sticky top-0 z-10 flex h-14 items-center justify-between gap-2 border-b border-slate-200 bg-white/90 px-3 sm:gap-4 sm:px-6 backdrop-blur dark:border-slate-800 dark:bg-slate-900/90">
          <div className="flex min-w-0 items-center gap-2 sm:gap-3">
            <button
              onClick={() => setOpen(true)}
              className="rounded-lg p-2 text-slate-600 hover:bg-slate-100 md:hidden dark:text-slate-300 dark:hover:bg-slate-800"
              aria-label="Ouvrir le menu"
              aria-expanded={open}
            >
              <Menu className="h-5 w-5" />
            </button>
            {me.tenants.length > 1 ? (
              <select
                aria-label="Société active"
                value={me.active_tenant_id ?? ''}
                onChange={(e) => switchTenant.mutate(e.target.value)}
                className="rounded-lg border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
              >
                {me.tenants.map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name}
                  </option>
                ))}
              </select>
            ) : (
              <span className="truncate text-sm font-medium">
                {active?.name ?? 'Administration plateforme'}
              </span>
            )}
            {me.role && (
              <span className="hidden rounded-full bg-slate-100 px-2 py-0.5 text-xs sm:inline text-slate-600 dark:bg-slate-800 dark:text-slate-300">
                {t(`role.${me.role}`)}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <ThemeSwitch />
            <span className="hidden text-sm text-slate-600 sm:inline dark:text-slate-300">
              {me.user.full_name}
            </span>
            <button
              onClick={() => logout.mutate()}
              className="flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
            >
              <LogOut className="h-4 w-4" aria-hidden />{' '}
              <span className="hidden sm:inline">{t('common.logout')}</span>
            </button>
          </div>
        </header>
        <main className="mx-auto w-full max-w-6xl flex-1 p-4 sm:p-6">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
