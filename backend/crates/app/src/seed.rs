//! Données de démonstration. TOUTES les valeurs sont fictives (noms, salaires, taux).

use crate::auth::password;
use anyhow::Result;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

pub const DEMO_PASSWORD: &str = "Demo-Passw0rd-2026";

async fn user(pool: &PgPool, tenant: Uuid, email: &str, name: &str, role: &str) -> Result<Uuid> {
    let hash = password::hash(DEMO_PASSWORD)?;
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO users (email, password_hash, full_name) VALUES ($1,$2,$3) RETURNING id",
    )
    .bind(email)
    .bind(hash)
    .bind(name)
    .fetch_one(pool)
    .await?;
    sqlx::query("INSERT INTO memberships (user_id, tenant_id, role) VALUES ($1,$2,$3)")
        .bind(id)
        .bind(tenant)
        .bind(role)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn demo(pool: &PgPool) -> Result<()> {
    if sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM tenants WHERE slug = 'demo'")
        .fetch_one(pool)
        .await?
        .0
        > 0
    {
        anyhow::bail!("la société « demo » existe déjà");
    }
    let (tenant,): (Uuid,) = sqlx::query_as(
        "INSERT INTO tenants (name, slug, currency) VALUES ('Société Démo (fictive)', 'demo', 'USD') RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    let hr = user(pool, tenant, "rh@demo.test", "Hélène RH (démo)", "hr_admin").await?;
    let _ = hr;
    user(
        pool,
        tenant,
        "paie1@demo.test",
        "Pierre Paie (démo)",
        "payroll_manager",
    )
    .await?;
    user(
        pool,
        tenant,
        "paie2@demo.test",
        "Paula Paie (démo)",
        "payroll_manager",
    )
    .await?;
    let manager_user = user(
        pool,
        tenant,
        "manager@demo.test",
        "Marc Manager (démo)",
        "manager",
    )
    .await?;
    let employee_user = user(
        pool,
        tenant,
        "employe@demo.test",
        "Eva Employée (démo)",
        "employee",
    )
    .await?;
    user(
        pool,
        tenant,
        "audit@demo.test",
        "Alain Auditeur (démo)",
        "auditor",
    )
    .await?;

    let (site,): (Uuid,) = sqlx::query_as(
        "INSERT INTO sites (tenant_id, name, city) VALUES ($1,'Siège','Lubumbashi') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await?;
    let (dept_ops,): (Uuid,) = sqlx::query_as(
        "INSERT INTO departments (tenant_id, name) VALUES ($1,'Opérations') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await?;
    let (dept_adm,): (Uuid,) = sqlx::query_as(
        "INSERT INTO departments (tenant_id, name) VALUES ($1,'Administration') RETURNING id",
    )
    .bind(tenant)
    .fetch_one(pool)
    .await?;
    let (pos_mgr,): (Uuid,) = sqlx::query_as("INSERT INTO positions (tenant_id, title, level) VALUES ($1,'Chef d''équipe','Cadre') RETURNING id").bind(tenant).fetch_one(pool).await?;
    let (pos_agent,): (Uuid,) = sqlx::query_as("INSERT INTO positions (tenant_id, title, level) VALUES ($1,'Agent','Exécution') RETURNING id").bind(tenant).fetch_one(pool).await?;

    let emp = |m: &'static str, f: &'static str, l: &'static str| (m, f, l);
    let (mgr_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO employees (tenant_id, matricule, first_name, last_name, email, hire_date, site_id, department_id, position_id, user_id)
         VALUES ($1,'M-001','Marc','Manager','manager@demo.test','2022-01-10',$2,$3,$4,$5) RETURNING id",
    ).bind(tenant).bind(site).bind(dept_ops).bind(pos_mgr).bind(manager_user).fetch_one(pool).await?;
    sqlx::query("INSERT INTO employee_compensation (tenant_id, employee_id, base_salary, currency, effective_from) VALUES ($1,$2,2500,'USD','2022-01-10')").bind(tenant).bind(mgr_id).execute(pool).await?;

    let team = [
        emp("M-002", "Eva", "Employée"),
        emp("M-003", "Joseph", "Kasongo"),
        emp("M-004", "Grâce", "Mutombo"),
        emp("M-005", "David", "Ilunga"),
    ];
    let (annual,): (Uuid,) =
        sqlx::query_as("SELECT id FROM leave_types WHERE tenant_id = $1 AND code = 'ANNUAL'")
            .bind(tenant)
            .fetch_optional(pool)
            .await?
            .unwrap_or_else(|| (Uuid::nil(),));
    let annual = if annual.is_nil() {
        sqlx::query_as::<_, (Uuid,)>("INSERT INTO leave_types (tenant_id, code, name, days_per_year, requires_balance) VALUES ($1,'ANNUAL','Congé annuel',20,TRUE) RETURNING id").bind(tenant).fetch_one(pool).await?.0
    } else {
        sqlx::query("UPDATE leave_types SET days_per_year = 20 WHERE id = $1")
            .bind(annual)
            .execute(pool)
            .await?;
        annual
    };
    for (i, (m, f, l)) in team.iter().enumerate() {
        let user_id = if *m == "M-002" {
            Some(employee_user)
        } else {
            None
        };
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO employees (tenant_id, matricule, first_name, last_name, email, hire_date, site_id, department_id, position_id, manager_id, user_id)
             VALUES ($1,$2,$3,$4,$5,'2023-03-01',$6,$7,$8,$9,$10) RETURNING id",
        )
        .bind(tenant).bind(m).bind(f).bind(l).bind(format!("{}@demo.test", l.to_lowercase())).bind(site)
        .bind(if i % 2 == 0 { dept_ops } else { dept_adm }).bind(pos_agent).bind(mgr_id).bind(user_id)
        .fetch_one(pool).await?;
        sqlx::query("INSERT INTO employee_compensation (tenant_id, employee_id, base_salary, currency, effective_from) VALUES ($1,$2,$3,'USD','2023-03-01')")
            .bind(tenant).bind(id).bind(rust_decimal::Decimal::from(800 + (i as i64) * 50)).execute(pool).await?;
        sqlx::query("INSERT INTO leave_balances (tenant_id, employee_id, leave_type_id, year, entitled) VALUES ($1,$2,$3,2026,20)").bind(tenant).bind(id).bind(annual).execute(pool).await?;
    }
    sqlx::query("INSERT INTO leave_balances (tenant_id, employee_id, leave_type_id, year, entitled) VALUES ($1,$2,$3,2026,20)").bind(tenant).bind(mgr_id).bind(annual).execute(pool).await?;

    // Jeu de règles DÉMO : valeurs inventées, jamais approuvable. Ne correspond à aucune législation.
    let params = json!({
        "rounding_scale": 2,
        "rounding_mode": "half_up",
        "employee_contributions": [{"code": "DEMO_SAL", "label": "Cotisation démo salarié (FICTIVE)", "rate": "0.04", "base": "gross", "ceiling": null}],
        "employer_contributions": [{"code": "DEMO_PAT", "label": "Cotisation démo employeur (FICTIVE)", "rate": "0.08", "base": "gross", "ceiling": null}],
        "tax_base_after_employee_contributions": true,
        "tax": {"code": "DEMO_IMP", "label": "Impôt démo (FICTIF)", "brackets": [
            {"upto": "500", "rate": "0.00"}, {"upto": "1500", "rate": "0.10"}, {"upto": null, "rate": "0.20"}]}
    });
    sqlx::query("INSERT INTO payroll_rule_sets (tenant_id, country, name, version, effective_from, params, source, is_demo) VALUES ($1,'CD','Profil DÉMO fictif',1,'2026-01-01',$2,'AUCUNE — valeurs fictives de démonstration',TRUE)")
        .bind(tenant).bind(params).execute(pool).await?;
    Ok(())
}
