//! Rôles et matrice de permissions (RBAC). Appliquée côté serveur uniquement.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    HrAdmin,
    HrManager,
    PayrollManager,
    Manager,
    Employee,
    Auditor,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::HrAdmin => "hr_admin",
            Role::HrManager => "hr_manager",
            Role::PayrollManager => "payroll_manager",
            Role::Manager => "manager",
            Role::Employee => "employee",
            Role::Auditor => "auditor",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "hr_admin" => Role::HrAdmin,
            "hr_manager" => Role::HrManager,
            "payroll_manager" => Role::PayrollManager,
            "manager" => Role::Manager,
            "employee" => Role::Employee,
            "auditor" => Role::Auditor,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    UsersManage,
    OrgRead,
    OrgManage,
    EmployeesRead,
    EmployeesManage,
    ContractsRead,
    ContractsManage,
    DocumentsRead,
    DocumentsManage,
    CompensationRead,
    CompensationManage,
    LeaveRequestOwn,
    LeaveApprove,
    LeaveReadAll,
    LeaveTypesManage,
    PayrollRead,
    PayrollManage,
    PayrollRulesManage,
    PayslipReadOwn,
    AttendanceClockOwn,
    AttendanceRead,
    AttendanceManage,
    ReportsRead,
    AuditRead,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::UsersManage => "users_manage",
            Permission::OrgRead => "org_read",
            Permission::OrgManage => "org_manage",
            Permission::EmployeesRead => "employees_read",
            Permission::EmployeesManage => "employees_manage",
            Permission::ContractsRead => "contracts_read",
            Permission::ContractsManage => "contracts_manage",
            Permission::DocumentsRead => "documents_read",
            Permission::DocumentsManage => "documents_manage",
            Permission::CompensationRead => "compensation_read",
            Permission::CompensationManage => "compensation_manage",
            Permission::LeaveRequestOwn => "leave_request_own",
            Permission::LeaveApprove => "leave_approve",
            Permission::LeaveReadAll => "leave_read_all",
            Permission::LeaveTypesManage => "leave_types_manage",
            Permission::PayrollRead => "payroll_read",
            Permission::PayrollManage => "payroll_manage",
            Permission::PayrollRulesManage => "payroll_rules_manage",
            Permission::PayslipReadOwn => "payslip_read_own",
            Permission::AttendanceClockOwn => "attendance_clock_own",
            Permission::AttendanceRead => "attendance_read",
            Permission::AttendanceManage => "attendance_manage",
            Permission::ReportsRead => "reports_read",
            Permission::AuditRead => "audit_read",
        }
    }
}

use Permission::*;

pub fn permissions_for(role: Role) -> &'static [Permission] {
    match role {
        Role::HrAdmin => &[
            UsersManage,
            OrgRead,
            OrgManage,
            EmployeesRead,
            EmployeesManage,
            ContractsRead,
            ContractsManage,
            DocumentsRead,
            DocumentsManage,
            CompensationRead,
            CompensationManage,
            LeaveRequestOwn,
            LeaveApprove,
            LeaveReadAll,
            LeaveTypesManage,
            PayslipReadOwn,
            AttendanceClockOwn,
            AttendanceRead,
            AttendanceManage,
            ReportsRead,
            AuditRead,
        ],
        Role::HrManager => &[
            OrgRead,
            OrgManage,
            EmployeesRead,
            EmployeesManage,
            ContractsRead,
            ContractsManage,
            DocumentsRead,
            DocumentsManage,
            LeaveRequestOwn,
            LeaveApprove,
            LeaveReadAll,
            LeaveTypesManage,
            PayslipReadOwn,
            AttendanceClockOwn,
            AttendanceRead,
            AttendanceManage,
            ReportsRead,
        ],
        Role::PayrollManager => &[
            OrgRead,
            EmployeesRead,
            CompensationRead,
            CompensationManage,
            LeaveReadAll,
            LeaveRequestOwn,
            PayrollRead,
            PayrollManage,
            PayrollRulesManage,
            PayslipReadOwn,
            AttendanceClockOwn,
            AttendanceRead,
            ReportsRead,
        ],
        Role::Manager => &[
            OrgRead,
            EmployeesRead,
            LeaveRequestOwn,
            LeaveApprove,
            PayslipReadOwn,
            AttendanceClockOwn,
            AttendanceRead,
        ],
        Role::Employee => &[OrgRead, LeaveRequestOwn, PayslipReadOwn, AttendanceClockOwn],
        Role::Auditor => &[
            OrgRead,
            EmployeesRead,
            ContractsRead,
            LeaveReadAll,
            PayrollRead,
            AttendanceRead,
            ReportsRead,
            AuditRead,
        ],
    }
}

pub fn has_permission(role: Role, perm: Permission) -> bool {
    permissions_for(role).contains(&perm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn employee_cannot_manage_payroll_or_see_compensation() {
        assert!(!has_permission(Role::Employee, PayrollManage));
        assert!(!has_permission(Role::Employee, CompensationRead));
        assert!(!has_permission(Role::Employee, EmployeesRead));
    }

    #[test]
    fn auditor_is_read_only() {
        assert!(!has_permission(Role::Auditor, EmployeesManage));
        assert!(!has_permission(Role::Auditor, PayrollManage));
        assert!(has_permission(Role::Auditor, AuditRead));
    }

    #[test]
    fn documents_are_hr_only() {
        for r in [Role::HrAdmin, Role::HrManager] {
            assert!(has_permission(r, DocumentsRead) && has_permission(r, DocumentsManage));
        }
        for r in [
            Role::PayrollManager,
            Role::Manager,
            Role::Employee,
            Role::Auditor,
        ] {
            assert!(!has_permission(r, DocumentsRead));
            assert!(!has_permission(r, DocumentsManage));
        }
    }

    #[test]
    fn contracts_are_hr_only() {
        assert!(has_permission(Role::HrAdmin, ContractsManage));
        assert!(!has_permission(Role::Employee, ContractsRead));
        assert!(!has_permission(Role::Manager, ContractsRead));
        assert!(!has_permission(Role::Auditor, ContractsManage));
    }

    #[test]
    fn attendance_permissions_are_scoped_by_role() {
        for r in [
            Role::HrAdmin,
            Role::HrManager,
            Role::PayrollManager,
            Role::Manager,
            Role::Employee,
        ] {
            assert!(has_permission(r, AttendanceClockOwn));
        }
        assert!(!has_permission(Role::Auditor, AttendanceClockOwn));
        assert!(!has_permission(Role::Employee, AttendanceRead));
        assert!(has_permission(Role::Manager, AttendanceRead));
        assert!(!has_permission(Role::Manager, AttendanceManage));
        assert!(!has_permission(Role::Auditor, AttendanceManage));
        assert!(has_permission(Role::HrManager, AttendanceManage));
    }

    #[test]
    fn roles_roundtrip() {
        for r in [
            Role::HrAdmin,
            Role::HrManager,
            Role::PayrollManager,
            Role::Manager,
            Role::Employee,
            Role::Auditor,
        ] {
            assert_eq!(Role::parse(r.as_str()), Some(r));
        }
        assert_eq!(Role::parse("root"), None);
    }
}
