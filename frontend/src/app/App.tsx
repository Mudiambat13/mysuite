import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { useAuth } from './auth'
import Layout from '../components/Layout'
import { Spinner } from '../components/ui'
import LoginPage from '../features/auth/LoginPage'
import ResetPasswordPage from '../features/auth/ResetPasswordPage'
import SecurityPage from '../features/security/SecurityPage'
import DashboardPage from '../features/dashboard/DashboardPage'
import EmployeesPage from '../features/employees/EmployeesPage'
import EmployeeDetailPage from '../features/employees/EmployeeDetailPage'
import OrganizationPage from '../features/organization/OrganizationPage'
import LeavePage from '../features/leave/LeavePage'
import PayrollPage from '../features/payroll/PayrollPage'
import RunDetailPage from '../features/payroll/RunDetailPage'
import SmigPage from '../features/payroll/SmigPage'
import MyPayslipsPage from '../features/payroll/MyPayslipsPage'
import ContractsPage from '../features/contracts/ContractsPage'
import AttendancePage from '../features/attendance/AttendancePage'
import ReportsPage from '../features/reports/ReportsPage'
import UsersPage from '../features/admin/UsersPage'
import AuditPage from '../features/admin/AuditPage'
import PlatformPage from '../features/admin/PlatformPage'

function Protected({
  perm,
  platform,
  children,
}: {
  perm?: string
  platform?: boolean
  children: React.ReactNode
}) {
  const { me, can } = useAuth()
  // Garde d'affichage uniquement : le serveur applique les vraies autorisations.
  if (platform && !me?.user.is_platform_admin) return <Navigate to="/" replace />
  if (perm && !can(perm)) return <Navigate to="/" replace />
  return <>{children}</>
}

function Shell() {
  const { me, loading } = useAuth()
  const location = useLocation()
  if (loading) return <Spinner />
  if (!me) return <Navigate to="/login" replace />
  // Société qui impose le MFA : seul l'écran de configuration est accessible tant qu'il n'est pas activé.
  if (me.mfa_enrollment_required && location.pathname !== '/security')
    return <Navigate to="/security" replace />
  return <Layout />
}

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/reset-password" element={<ResetPasswordPage />} />
      <Route element={<Shell />}>
        <Route index element={<DashboardPage />} />
        <Route
          path="employees"
          element={
            <Protected perm="employees_read">
              <EmployeesPage />
            </Protected>
          }
        />
        <Route
          path="employees/:id"
          element={
            <Protected perm="employees_read">
              <EmployeeDetailPage />
            </Protected>
          }
        />
        <Route
          path="contracts"
          element={
            <Protected perm="contracts_read">
              <ContractsPage />
            </Protected>
          }
        />
        <Route
          path="organization"
          element={
            <Protected perm="org_read">
              <OrganizationPage />
            </Protected>
          }
        />
        <Route
          path="leave"
          element={
            <Protected perm="leave_request_own">
              <LeavePage />
            </Protected>
          }
        />
        <Route
          path="payroll"
          element={
            <Protected perm="payroll_read">
              <PayrollPage />
            </Protected>
          }
        />
        <Route
          path="payroll/smig"
          element={
            <Protected perm="compensation_read">
              <SmigPage />
            </Protected>
          }
        />
        <Route
          path="payroll/runs/:id"
          element={
            <Protected perm="payroll_read">
              <RunDetailPage />
            </Protected>
          }
        />
        <Route
          path="payslips"
          element={
            <Protected perm="payslip_read_own">
              <MyPayslipsPage />
            </Protected>
          }
        />
        <Route
          path="attendance"
          element={
            <Protected perm="attendance_clock_own">
              <AttendancePage />
            </Protected>
          }
        />
        <Route
          path="reports"
          element={
            <Protected perm="reports_read">
              <ReportsPage />
            </Protected>
          }
        />
        <Route path="security" element={<SecurityPage />} />
        <Route
          path="users"
          element={
            <Protected perm="users_manage">
              <UsersPage />
            </Protected>
          }
        />
        <Route
          path="audit"
          element={
            <Protected perm="audit_read">
              <AuditPage />
            </Protected>
          }
        />
        <Route
          path="platform"
          element={
            <Protected platform>
              <PlatformPage />
            </Protected>
          }
        />
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
