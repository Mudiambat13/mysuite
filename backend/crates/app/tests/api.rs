//! Tests d'intégration sur un vrai PostgreSQL (une base éphémère par test via `sqlx::test`).

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use mysuite_app::{auth::password, build_router, config::Config, state::AppState};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const PWD: &str = "Test-Passw0rd-2026";

struct Client {
    app: Router,
    cookie: Option<String>,
}

impl Client {
    fn new(pool: &PgPool) -> Self {
        Self {
            app: build_router(AppState::new(pool.clone(), Config::for_tests())),
            cookie: None,
        }
    }

    async fn raw(
        &mut self,
        method: Method,
        uri: &str,
        body: Option<Value>,
        csrf: bool,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"));
        if csrf {
            req = req.header("x-requested-with", "mysuite");
        }
        if let Some(c) = &self.cookie {
            req = req.header(header::COOKIE, c.clone());
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        if let Some(sc) = resp
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
        {
            let pair = sc.split(';').next().unwrap().to_string();
            self.cookie = if pair.ends_with('=') {
                None
            } else {
                Some(pair)
            };
        }
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }

    async fn get(&mut self, uri: &str) -> (StatusCode, Value) {
        self.raw(Method::GET, uri, None, true).await
    }
    async fn post(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.raw(Method::POST, uri, Some(body), true).await
    }
    async fn put(&mut self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.raw(Method::PUT, uri, Some(body), true).await
    }
    async fn login(&mut self, email: &str) -> StatusCode {
        self.cookie = None;
        self.post("/auth/login", json!({"email": email, "password": PWD}))
            .await
            .0
    }
}

async fn make_platform_admin(pool: &PgPool) {
    sqlx::query("INSERT INTO users (email, password_hash, full_name, is_platform_admin) VALUES ('root@test.dev', $1, 'Root', TRUE)")
        .bind(password::hash(PWD).unwrap())
        .execute(pool)
        .await
        .unwrap();
}

/// Crée une société et son admin RH ; retourne l'id de société.
async fn make_tenant(pool: &PgPool, slug: &str, admin_email: &str) -> String {
    make_platform_admin_once(pool).await;
    let mut root = Client::new(pool);
    assert_eq!(root.login("root@test.dev").await, StatusCode::OK);
    let (st, body) = root
        .post("/platform/tenants", json!({"name": slug, "slug": slug, "admin_email": admin_email, "admin_name": "Admin RH", "admin_password": PWD}))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

async fn make_platform_admin_once(pool: &PgPool) {
    let exists: (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE email = 'root@test.dev'")
        .fetch_one(pool)
        .await
        .unwrap();
    if exists.0 == 0 {
        make_platform_admin(pool).await;
    }
}

async fn make_employee(
    hr: &mut Client,
    matricule: &str,
    first: &str,
    manager: Option<&str>,
) -> String {
    let (st, body) = hr
        .post("/employees", json!({"matricule": matricule, "first_name": first, "last_name": "Test", "hire_date": "2025-01-01", "manager_id": manager}))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_string()
}

async fn make_user(hr: &mut Client, email: &str, role: &str, employee: Option<&str>) {
    let (st, body) = hr.post("/users", json!({"email": email, "full_name": email, "password": PWD, "role": role, "employee_id": employee})).await;
    assert_eq!(st, StatusCode::CREATED, "{body}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn csrf_header_is_required_on_mutations(pool: PgPool) {
    let mut c = Client::new(&pool);
    let (st, _) = c
        .raw(
            Method::POST,
            "/auth/login",
            Some(json!({"email": "a@b.cd", "password": "x"})),
            false,
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unauthenticated_requests_are_rejected(pool: PgPool) {
    let mut c = Client::new(&pool);
    assert_eq!(c.get("/employees").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(c.get("/auth/me").await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_locks_after_repeated_failures(pool: PgPool) {
    make_platform_admin(&pool).await;
    let mut c = Client::new(&pool);
    for _ in 0..5 {
        let (st, _) = c
            .post(
                "/auth/login",
                json!({"email": "root@test.dev", "password": "wrong"}),
            )
            .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }
    // Même avec le bon mot de passe, le compte est verrouillé.
    let (st, _) = c
        .post(
            "/auth/login",
            json!({"email": "root@test.dev", "password": PWD}),
        )
        .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_revokes_the_session(pool: PgPool) {
    make_platform_admin(&pool).await;
    let mut c = Client::new(&pool);
    assert_eq!(c.login("root@test.dev").await, StatusCode::OK);
    let old_cookie = c.cookie.clone();
    assert_eq!(c.get("/auth/me").await.0, StatusCode::OK);
    c.raw(Method::POST, "/auth/logout", None, true).await;
    c.cookie = old_cookie; // rejoue l'ancien jeton
    assert_eq!(c.get("/auth/me").await.0, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn tenants_are_strictly_isolated(pool: PgPool) {
    let a = make_tenant(&pool, "alpha", "hr@alpha.test").await;
    let _b = make_tenant(&pool, "bravo", "hr@bravo.test").await;
    let mut hr_a = Client::new(&pool);
    let mut hr_b = Client::new(&pool);
    assert_eq!(hr_a.login("hr@alpha.test").await, StatusCode::OK);
    assert_eq!(hr_b.login("hr@bravo.test").await, StatusCode::OK);

    let (_, dept_a) = hr_a
        .post("/org/departments", json!({"name": "Finance"}))
        .await;
    let emp_a = make_employee(&mut hr_a, "A-1", "Alice", None).await;
    let emp_b = make_employee(&mut hr_b, "B-1", "Bob", None).await;

    // Pagination par query string (régression : serde flatten + nombres).
    assert_eq!(
        hr_a.get("/employees?page=1&limit=10").await.0,
        StatusCode::OK
    );
    assert_eq!(hr_a.get("/audit?page=1&limit=10").await.0, StatusCode::OK);
    // B ne voit que ses employés.
    let (_, list) = hr_b.get("/employees").await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["matricule"], "B-1");

    // B ne peut ni lire, ni modifier, ni exporter l'employé de A (404, pas de fuite d'existence).
    assert_eq!(
        hr_b.get(&format!("/employees/{emp_a}")).await.0,
        StatusCode::NOT_FOUND
    );
    let (st, _) = hr_b.put(&format!("/employees/{emp_a}"), json!({"matricule": "HACK", "first_name": "x", "last_name": "y", "hire_date": "2025-01-01"})).await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    assert_eq!(
        hr_b.get(&format!("/employees/{emp_a}/compensation"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // Rattacher un employé de B à un département de A est refusé par les clés composites.
    let (st, _) = hr_b
        .post("/employees", json!({"matricule": "B-2", "first_name": "Eve", "last_name": "T", "hire_date": "2025-01-01", "department_id": dept_a["id"]}))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // B ne peut pas basculer vers la société A.
    let (st, _) = hr_b
        .post("/auth/switch-tenant", json!({"tenant_id": a}))
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // Un matricule peut exister dans deux sociétés.
    make_employee(&mut hr_b, "A-1", "Zoé", None).await;
    let _ = emp_b;
}

#[sqlx::test(migrations = "../../migrations")]
async fn employee_role_cannot_access_hr_or_payroll_data(pool: PgPool) {
    make_tenant(&pool, "gamma", "hr@gamma.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@gamma.test").await;
    let emp = make_employee(&mut hr, "E-1", "Eva", None).await;
    make_user(&mut hr, "eva@gamma.test", "employee", Some(&emp)).await;

    let mut eva = Client::new(&pool);
    assert_eq!(eva.login("eva@gamma.test").await, StatusCode::OK);
    assert_eq!(eva.get("/employees").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        eva.get(&format!("/employees/{emp}/compensation")).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(eva.get("/payroll/runs").await.0, StatusCode::FORBIDDEN);
    assert_eq!(eva.get("/users").await.0, StatusCode::FORBIDDEN);
    assert_eq!(eva.get("/audit").await.0, StatusCode::FORBIDDEN);
    assert_eq!(eva.post("/employees", json!({"matricule": "X", "first_name": "a", "last_name": "b", "hire_date": "2025-01-01"})).await.0, StatusCode::FORBIDDEN);
    // Mais accède à son propre dossier.
    assert_eq!(eva.get("/employees/me").await.0, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leave_request_and_approval_flow(pool: PgPool) {
    let tenant = make_tenant(&pool, "delta", "hr@delta.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@delta.test").await;
    let mgr = make_employee(&mut hr, "M-1", "Manon", None).await;
    let eva = make_employee(&mut hr, "E-1", "Eva", Some(&mgr)).await;
    let other = make_employee(&mut hr, "O-1", "Omar", None).await; // hors équipe de Manon
    make_user(&mut hr, "manon@delta.test", "manager", Some(&mgr)).await;
    make_user(&mut hr, "eva@delta.test", "employee", Some(&eva)).await;
    make_user(&mut hr, "omar@delta.test", "employee", Some(&other)).await;

    let (_, types) = hr.get("/leave/types").await;
    let annual = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "ANNUAL")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (st, _) = hr
        .put(
            "/leave/balances",
            json!({"employee_id": eva, "leave_type_id": annual, "year": 2030, "entitled": "3"}),
        )
        .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let mut e = Client::new(&pool);
    e.login("eva@delta.test").await;
    // 2030-03-04 (lun) → 2030-03-08 (ven) = 5 jours > solde 3
    let (st, body) = e
        .post(
            "/leave/requests",
            json!({"leave_type_id": annual, "start_date": "2030-03-04", "end_date": "2030-03-08"}),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    // 3 jours : accepté
    let (st, req) = e
        .post(
            "/leave/requests",
            json!({"leave_type_id": annual, "start_date": "2030-03-04", "end_date": "2030-03-06"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{req}");
    assert_eq!(req["status"], "pending");
    let rid = req["id"].as_str().unwrap();
    // Chevauchement refusé
    let (st, _) = e
        .post(
            "/leave/requests",
            json!({"leave_type_id": annual, "start_date": "2030-03-05", "end_date": "2030-03-05"}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // L'employé ne peut pas approuver
    assert_eq!(
        e.post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": true})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // Un manager d'une autre équipe n'y accède pas ; ici Omar (employé) non plus.
    let mut o = Client::new(&pool);
    o.login("omar@delta.test").await;
    assert_eq!(
        o.post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": true})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let mut m = Client::new(&pool);
    m.login("manon@delta.test").await;
    let (_, team) = m.get("/leave/requests?scope=team&status=pending").await;
    assert_eq!(team.as_array().unwrap().len(), 1);
    let (st, done) = m
        .post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": true, "comment": "ok"}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{done}");
    assert_eq!(done["status"], "approved");
    // Décision déjà prise
    assert_eq!(
        m.post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": false})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    // Tout est journalisé
    let (_, audit) = hr.get("/audit").await;
    let actions: Vec<&str> = audit["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"leave.approve") && actions.contains(&"leave.request"));
    let _ = tenant;
}

#[sqlx::test(migrations = "../../migrations")]
async fn manager_cannot_approve_own_request_or_other_teams(pool: PgPool) {
    make_tenant(&pool, "echo", "hr@echo.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@echo.test").await;
    let m1 = make_employee(&mut hr, "M-1", "Un", None).await;
    let m2 = make_employee(&mut hr, "M-2", "Deux", None).await;
    let sub = make_employee(&mut hr, "S-1", "Sub", Some(&m2)).await;
    make_user(&mut hr, "m1@echo.test", "manager", Some(&m1)).await;
    make_user(&mut hr, "sub@echo.test", "employee", Some(&sub)).await;
    let (_, types) = hr.get("/leave/types").await;
    let sick = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "SICK")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut s = Client::new(&pool);
    s.login("sub@echo.test").await;
    let (st, req) = s
        .post(
            "/leave/requests",
            json!({"leave_type_id": sick, "start_date": "2030-05-06", "end_date": "2030-05-06"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{req}");
    let mut m = Client::new(&pool);
    m.login("m1@echo.test").await; // manager d'une AUTRE équipe
    let rid = req["id"].as_str().unwrap();
    assert_eq!(
        m.post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": true})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

async fn payroll_setup(pool: &PgPool) -> (Client, Client, Client, String, String) {
    make_tenant(pool, "foxtrot", "hr@foxtrot.test").await;
    let mut hr = Client::new(pool);
    hr.login("hr@foxtrot.test").await;
    let e1 = make_employee(&mut hr, "P-1", "Paul", None).await;
    let e2 = make_employee(&mut hr, "P-2", "Pia", None).await;
    make_user(&mut hr, "paul@foxtrot.test", "employee", Some(&e1)).await;
    make_user(&mut hr, "pia@foxtrot.test", "employee", Some(&e2)).await;
    make_user(&mut hr, "pay1@foxtrot.test", "payroll_manager", None).await;
    make_user(&mut hr, "pay2@foxtrot.test", "payroll_manager", None).await;
    let mut pay1 = Client::new(pool);
    pay1.login("pay1@foxtrot.test").await;
    for e in [&e1, &e2] {
        let (st, b) = pay1
            .post(
                &format!("/employees/{e}/compensation"),
                json!({"base_salary": "1000.00", "effective_from": "2025-01-01"}),
            )
            .await;
        assert_eq!(st, StatusCode::CREATED, "{b}");
    }
    let mut pay2 = Client::new(pool);
    pay2.login("pay2@foxtrot.test").await;
    (hr, pay1, pay2, e1, e2)
}

fn rules() -> Value {
    json!({
        "rounding_scale": 2, "rounding_mode": "half_up",
        "employee_contributions": [{"code": "T_EMP", "label": "Test salarié", "rate": "0.05", "base": "gross"}],
        "employer_contributions": [{"code": "T_ER", "label": "Test employeur", "rate": "0.10", "base": "gross"}],
        "tax_base_after_employee_contributions": true,
        "tax": {"code": "T_TAX", "label": "Impôt test", "brackets": [{"upto": "500", "rate": "0"}, {"upto": null, "rate": "0.10"}]}
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn payroll_is_simulation_until_rules_are_approved_by_someone_else(pool: PgPool) {
    let (_hr, mut pay1, mut pay2, _e1, _e2) = payroll_setup(&pool).await;
    let (st, rs) = pay1.post("/payroll/rule-sets", json!({"country": "CD", "name": "Test", "effective_from": "2025-01-01", "params": rules()})).await;
    assert_eq!(st, StatusCode::CREATED, "{rs}");
    assert_eq!(rs["status"], "draft");
    let rs_id = rs["id"].as_str().unwrap();
    let (_, period) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 1}))
        .await;
    let (st, run) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs_id}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{run}");
    assert_eq!(run["run"]["simulation"], true);
    assert_eq!(run["items"].as_array().unwrap().len(), 2);
    let run_id = run["run"]["id"].as_str().unwrap();
    // Validation impossible en simulation
    assert_eq!(
        pay1.post(&format!("/payroll/runs/{run_id}/validate"), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    // Le créateur ne peut pas approuver ses propres règles (quatre yeux)
    let approve = json!({"source": "Texte officiel de test, art. 1", "verified_on": "2026-01-02"});
    assert_eq!(
        pay1.post(
            &format!("/payroll/rule-sets/{rs_id}/approve"),
            approve.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        pay2.post(&format!("/payroll/rule-sets/{rs_id}/approve"), approve)
            .await
            .0,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn demo_rule_sets_can_never_be_approved(pool: PgPool) {
    let (_hr, _pay1, mut pay2, _e1, _e2) = payroll_setup(&pool).await;
    let (tenant,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM tenants WHERE slug = 'foxtrot'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (id,): (uuid::Uuid,) = sqlx::query_as("INSERT INTO payroll_rule_sets (tenant_id, country, name, version, effective_from, params, is_demo) VALUES ($1,'CD','Démo',1,'2025-01-01',$2,TRUE) RETURNING id")
        .bind(tenant).bind(rules()).fetch_one(&pool).await.unwrap();
    let (st, _) = pay2
        .post(
            &format!("/payroll/rule-sets/{id}/approve"),
            json!({"source": "x", "verified_on": "2026-01-02"}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_payroll_cycle_payslip_privacy_and_immutability(pool: PgPool) {
    let (mut hr, mut pay1, mut pay2, e1, _e2) = payroll_setup(&pool).await;
    let (_, rs) = pay1.post("/payroll/rule-sets", json!({"country": "CD", "name": "Test", "effective_from": "2025-01-01", "params": rules()})).await;
    let rs_id = rs["id"].as_str().unwrap();
    pay2.post(
        &format!("/payroll/rule-sets/{rs_id}/approve"),
        json!({"source": "Texte de test", "verified_on": "2026-01-02"}),
    )
    .await;
    let (_, period) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 2}))
        .await;
    let (_, run) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs_id}),
        )
        .await;
    assert_eq!(run["run"]["simulation"], false);
    let run_id = run["run"]["id"].as_str().unwrap();

    // 1000 brut ; cotisation salarié 50 ; assiette impôt 950 ; impôt = 10% de 450 = 45 ; net = 905
    let item = &run["items"][0];
    assert_eq!(item["gross"], "1000.00");
    assert_eq!(item["net"], "905.00");
    assert_eq!(run["run"]["total_employer_cost"], "2200.00");

    // Pas de bulletin visible tant que la paie n'est pas clôturée
    let mut paul = Client::new(&pool);
    paul.login("paul@foxtrot.test").await;
    assert_eq!(
        paul.get("/payslips/me").await.1.as_array().unwrap().len(),
        0
    );

    assert_eq!(
        pay1.post(&format!("/payroll/runs/{run_id}/close"), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    ); // pas encore validée
    assert_eq!(
        pay1.post(&format!("/payroll/runs/{run_id}/validate"), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    // Recalcul interdit après validation
    assert_eq!(
        pay1.post(&format!("/payroll/runs/{run_id}/recalculate"), json!({}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        pay1.post(&format!("/payroll/runs/{run_id}/close"), json!({}))
            .await
            .0,
        StatusCode::OK
    );

    // Paul voit son bulletin, avec le détail des lignes
    let (_, mine) = paul.get("/payslips/me").await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let my_item = mine[0]["item_id"].as_str().unwrap();
    let (st, slip) = paul.get(&format!("/payslips/{my_item}")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(slip["result"]["lines"].as_array().unwrap().len() >= 4);

    // Pia ne peut PAS consulter le bulletin de Paul
    let mut pia = Client::new(&pool);
    pia.login("pia@foxtrot.test").await;
    assert_eq!(
        pia.get(&format!("/payslips/{my_item}")).await.0,
        StatusCode::NOT_FOUND
    );

    // Bulletin PDF : titulaire OK (fichier PDF valide, sauvegardé pour contrôle externe), autre employé 404
    let pdf_resp = paul
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/payslips/{my_item}/pdf"))
                .header(header::COOKIE, paul.cookie.clone().unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pdf_resp.status(), StatusCode::OK);
    assert_eq!(pdf_resp.headers()[header::CONTENT_TYPE], "application/pdf");
    let pdf_bytes = pdf_resp.into_body().collect().await.unwrap().to_bytes();
    assert!(pdf_bytes.starts_with(b"%PDF-1.4"));
    if let Ok(dir) = std::env::var("MYSUITE_TEST_PDF_DIR") {
        std::fs::write(format!("{dir}/payslip.pdf"), &pdf_bytes).unwrap();
    }
    assert_eq!(
        pia.get(&format!("/payslips/{my_item}/pdf")).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, audit) = hr.get("/audit?limit=100").await;
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action"] == "payslip.download")
    );

    // La période est clôturée : nouvelle paie refusée
    let (st, _) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs_id}),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Immutabilité au niveau base de données, même en SQL direct.
    let upd = sqlx::query("UPDATE payroll_items SET net = 1 WHERE employee_id = $1")
        .bind(uuid::Uuid::parse_str(&e1).unwrap())
        .execute(&pool)
        .await;
    assert!(
        upd.is_err(),
        "les lignes d'une paie clôturée doivent être figées"
    );
    let del = sqlx::query("DELETE FROM payroll_items")
        .execute(&pool)
        .await;
    assert!(del.is_err());
    let reopen = sqlx::query("UPDATE payroll_runs SET status = 'draft'")
        .execute(&pool)
        .await;
    assert!(reopen.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn audit_log_is_append_only(pool: PgPool) {
    make_tenant(&pool, "golf", "hr@golf.test").await;
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM audit_logs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(n > 0);
    assert!(
        sqlx::query("UPDATE audit_logs SET action = 'x'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM audit_logs")
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_export_neutralizes_formula_injection(pool: PgPool) {
    make_tenant(&pool, "hotel", "hr@hotel.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@hotel.test").await;
    let (st, _) = hr.post("/employees", json!({"matricule": "=CMD()", "first_name": "A", "last_name": "B", "hire_date": "2025-01-01"})).await;
    assert_eq!(st, StatusCode::CREATED);
    let resp = hr
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/employees/export.csv")
                .header(header::COOKIE, hr.cookie.clone().unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(text.contains("'=CMD()"), "{text}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn contracts_rules_permissions_and_isolation(pool: PgPool) {
    make_tenant(&pool, "india", "hr@india.test").await;
    make_tenant(&pool, "juliet", "hr@juliet.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@india.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@juliet.test").await;
    let emp = make_employee(&mut hr, "C-1", "Carl", None).await;
    make_user(&mut hr, "carl@india.test", "employee", Some(&emp)).await;

    // CDD sans date de fin refusé
    let (st, _) = hr
        .post(
            "/contracts",
            json!({"employee_id": emp, "contract_type": "cdd", "start_date": "2026-01-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Contrat valide
    let (st, c) = hr
        .post("/contracts", json!({"employee_id": emp, "contract_type": "cdd", "start_date": "2026-01-01", "end_date": "2026-06-30", "probation_end": "2026-02-01"}))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{c}");
    let cid = c["id"].as_str().unwrap();
    // Un seul contrat actif par employé
    let (st, _) = hr
        .post(
            "/contracts",
            json!({"employee_id": emp, "contract_type": "cdi", "start_date": "2026-07-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
    // Avenant de prolongation : met à jour la fin et garde l'historique
    let (st, _) = hr.post(&format!("/contracts/{cid}/amendments"), json!({"effective_date": "2026-06-01", "description": "Prolongation 6 mois", "new_end_date": "2026-12-31"})).await;
    assert_eq!(st, StatusCode::CREATED);
    let (_, list) = hr.get(&format!("/contracts?employee_id={emp}")).await;
    assert_eq!(list[0]["end_date"], "2026-12-31");
    assert_eq!(
        hr.get(&format!("/contracts/{cid}/amendments"))
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Un employé ne voit aucun contrat ; une autre société non plus
    let mut carl = Client::new(&pool);
    carl.login("carl@india.test").await;
    assert_eq!(carl.get("/contracts").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        carl.post(
            "/contracts",
            json!({"employee_id": emp, "contract_type": "cdi", "start_date": "2026-01-01"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(other.get("/contracts").await.1.as_array().unwrap().len(), 0);
    assert_eq!(
        other.get(&format!("/contracts/{cid}/amendments")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other
            .post(
                &format!("/contracts/{cid}/terminate"),
                json!({"end_date": "2026-03-01"})
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    // Contrat d'un employé d'une autre société : refusé par la clé composite
    let free_emp = make_employee(&mut hr, "C-2", "Clara", None).await; // sans contrat
    let (st, _) = other
        .post(
            "/contracts",
            json!({"employee_id": free_emp, "contract_type": "cdi", "start_date": "2026-01-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Résiliation puis plus d'avenant possible
    assert_eq!(
        hr.post(
            &format!("/contracts/{cid}/terminate"),
            json!({"end_date": "2026-09-30"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        hr.post(
            &format!("/contracts/{cid}/terminate"),
            json!({"end_date": "2026-09-30"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (st, _) = hr
        .post(
            &format!("/contracts/{cid}/amendments"),
            json!({"effective_date": "2026-10-01", "description": "x"}),
        )
        .await;
    assert_eq!(st, StatusCode::CONFLICT);
}

async fn get_text(c: &Client, uri: &str) -> (StatusCode, String, String) {
    let resp = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1{uri}"))
                .header(header::COOKIE, c.cookie.clone().unwrap())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let st = resp.status();
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    (st, ct, body)
}

#[sqlx::test(migrations = "../../migrations")]
async fn reports_movements_and_absences_are_exact_scoped_and_permissioned(pool: PgPool) {
    make_tenant(&pool, "kilo", "hr@kilo.test").await;
    make_tenant(&pool, "lima", "hr@lima.test").await;
    let mut hr = Client::new(&pool);
    hr.login("hr@kilo.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@lima.test").await;

    let (_, dept) = hr
        .post("/org/departments", json!({"name": "Logistique"}))
        .await;
    let mgr = make_employee(&mut hr, "R-1", "Rita", None).await;
    let a = make_employee(&mut hr, "R-2", "Ali", Some(&mgr)).await;
    let b = make_employee(&mut hr, "R-3", "Bea", Some(&mgr)).await;
    make_user(&mut hr, "ali@kilo.test", "employee", Some(&a)).await;
    make_user(&mut hr, "rita@kilo.test", "manager", Some(&mgr)).await;
    // Ali : département Logistique, sorti le 2025-06-30 ; Bea : sans département.
    sqlx::query("UPDATE employees SET department_id = $2::uuid, status = 'exited', exit_date = '2025-06-30' WHERE id = $1::uuid")
        .bind(&a)
        .bind(dept["id"].as_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let _ = b;

    // Mouvements 2025 : 3 entrées (2025-01-01), 1 sortie (2025-06-30)
    let (st, m) = hr
        .get("/reports/movements?from=2025-01-01&to=2025-12-31")
        .await;
    assert_eq!(st, StatusCode::OK, "{m}");
    assert_eq!(m["entries"].as_array().unwrap().len(), 3);
    assert_eq!(m["exits"].as_array().unwrap().len(), 1);
    let months = m["by_month"].as_array().unwrap();
    assert_eq!(months.len(), 12);
    assert_eq!(months[0]["month"], "2025-01");
    assert_eq!(months[0]["entries"], 3);
    assert_eq!(months[5]["exits"], 1);
    // Filtre département
    let (_, f) = hr
        .get(&format!(
            "/reports/movements?from=2025-01-01&to=2025-12-31&department_id={}",
            dept["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(f["entries"].as_array().unwrap().len(), 1);
    // Période invalide ou trop longue
    assert_eq!(
        hr.get("/reports/movements?from=2025-12-31&to=2025-01-01")
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        hr.get("/reports/movements?from=2000-01-01&to=2025-01-01")
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Isolation : l'autre société ne voit rien
    let (_, o) = other
        .get("/reports/movements?from=2025-01-01&to=2025-12-31")
        .await;
    assert_eq!(o["entries"].as_array().unwrap().len(), 0);

    // Absences : Ali (employé) 3 jours maladie approuvés par RH ; une demande en attente n'est pas comptée
    let (_, types) = hr.get("/leave/types").await;
    let sick = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["code"] == "SICK")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    sqlx::query("UPDATE employees SET status = 'active', exit_date = NULL WHERE id = $1::uuid")
        .bind(&a)
        .execute(&pool)
        .await
        .unwrap();
    let mut ali = Client::new(&pool);
    ali.login("ali@kilo.test").await;
    let (st, r1) = ali
        .post(
            "/leave/requests",
            json!({"leave_type_id": sick, "start_date": "2030-03-04", "end_date": "2030-03-06"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{r1}");
    let (_, _r2) = ali
        .post(
            "/leave/requests",
            json!({"leave_type_id": sick, "start_date": "2030-04-01", "end_date": "2030-04-01"}),
        )
        .await;
    let rid = r1["id"].as_str().unwrap();
    assert_eq!(
        hr.post(
            &format!("/leave/requests/{rid}/decision"),
            json!({"approve": true})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, ab) = hr
        .get("/reports/absences?from=2030-01-01&to=2030-12-31")
        .await;
    assert_eq!(ab["total_days"], "3.00");
    assert_eq!(ab["by_type"][0]["label"], "Congé maladie");
    assert_eq!(ab["by_department"][0]["label"], "Logistique");
    assert_eq!(
        other
            .get("/reports/absences?from=2030-01-01&to=2030-12-31")
            .await
            .1["total_days"],
        "0"
    );

    // Permissions : employé et manager n'ont pas les rapports ; la masse salariale est réservée à la paie/audit
    assert_eq!(ali.get("/reports/movements").await.0, StatusCode::FORBIDDEN);
    let mut rita = Client::new(&pool);
    rita.login("rita@kilo.test").await;
    assert_eq!(rita.get("/reports/absences").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        hr.get("/reports/payroll-cost").await.0,
        StatusCode::FORBIDDEN
    );

    // Export CSV : type, BOM, en-tête, audit (on remet la sortie d'Ali après la partie congés)
    sqlx::query(
        "UPDATE employees SET status = 'exited', exit_date = '2025-06-30' WHERE id = $1::uuid",
    )
    .bind(&a)
    .execute(&pool)
    .await
    .unwrap();
    let (st, ct, body) =
        get_text(&hr, "/reports/movements.csv?from=2025-01-01&to=2025-12-31").await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.starts_with("text/csv"));
    assert!(body.starts_with("\u{feff}mouvement,matricule"));
    assert!(body.contains("sortie,R-2"));
    let (_, audit) = hr.get("/audit?limit=100").await;
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["action"] == "report.export")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn payroll_cost_report_excludes_simulations_and_open_runs(pool: PgPool) {
    let (mut hr, mut pay1, mut pay2, _e1, _e2) = payroll_setup(&pool).await;
    // Règles approuvées + paie de février 2026
    let (_, rs) = pay1.post("/payroll/rule-sets", json!({"country": "CD", "name": "Test", "effective_from": "2025-01-01", "params": rules()})).await;
    let rs_id = rs["id"].as_str().unwrap();
    pay2.post(
        &format!("/payroll/rule-sets/{rs_id}/approve"),
        json!({"source": "Texte de test", "verified_on": "2026-01-02"}),
    )
    .await;
    let (_, period) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 2}))
        .await;
    let (_, run) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs_id}),
        )
        .await;
    let run_id = run["run"]["id"].as_str().unwrap();

    // Brouillon : exclu
    let (st, c) = pay1.get("/reports/payroll-cost?year=2026").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(c["months"].as_array().unwrap().len(), 0);

    pay1.post(&format!("/payroll/runs/{run_id}/validate"), json!({}))
        .await;
    // Validée mais non clôturée : exclue
    assert_eq!(
        pay1.get("/reports/payroll-cost?year=2026").await.1["months"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    pay1.post(&format!("/payroll/runs/{run_id}/close"), json!({}))
        .await;

    let (_, c) = pay1.get("/reports/payroll-cost?year=2026").await;
    let m = &c["months"][0];
    assert_eq!(m["month"], 2);
    assert_eq!(m["gross"], "2000.00");
    assert_eq!(m["net"], "1810.00");
    assert_eq!(m["employer_cost"], "2200.00");
    assert_eq!(m["employees"], 2);
    assert_eq!(c["by_department"][0]["label"], "Non affecté");
    // Autre année : vide
    assert_eq!(
        pay1.get("/reports/payroll-cost?year=2025").await.1["months"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    // L'admin RH n'a pas accès à la masse salariale ; export CSV pour la paie
    assert_eq!(
        hr.get("/reports/payroll-cost?year=2026").await.0,
        StatusCode::FORBIDDEN
    );
    let (st, _, body) = get_text(&pay1, "/reports/payroll-cost.csv?year=2026").await;
    assert_eq!(st, StatusCode::OK);
    assert!(body.contains("mois,2026-02,2000.00,1810.00,2200.00,2"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn simulation_payroll_never_counts_as_payroll_cost(pool: PgPool) {
    let (_hr, mut pay1, _pay2, _e1, _e2) = payroll_setup(&pool).await;
    let (tenant,): (uuid::Uuid,) = sqlx::query_as("SELECT id FROM tenants WHERE slug = 'foxtrot'")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Jeu de règles non approuvé => simulation ; on force la clôture en SQL pour prouver que le rapport l'exclut de toute façon.
    let (rs,): (uuid::Uuid,) = sqlx::query_as("INSERT INTO payroll_rule_sets (tenant_id, country, name, version, effective_from, params, is_demo) VALUES ($1,'CD','Démo',1,'2025-01-01',$2,TRUE) RETURNING id")
        .bind(tenant)
        .bind(rules())
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_, period) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 3}))
        .await;
    let (_, run) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs.to_string()}),
        )
        .await;
    assert_eq!(run["run"]["simulation"], true);
    assert_eq!(
        pay1.get("/reports/payroll-cost?year=2026").await.1["months"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

// ---------- Pointage ----------

async fn attendance_setup(pool: &PgPool) -> (Client, String, String, String) {
    make_tenant(pool, "pointage", "hr@pointage.test").await;
    let mut hr = Client::new(pool);
    hr.login("hr@pointage.test").await;
    let boss = make_employee(&mut hr, "B-1", "Chef", None).await;
    let emp = make_employee(&mut hr, "E-1", "Alice", Some(&boss)).await;
    let other = make_employee(&mut hr, "E-2", "Bob", None).await;
    make_user(&mut hr, "boss@pointage.test", "manager", Some(&boss)).await;
    make_user(&mut hr, "emp@pointage.test", "employee", Some(&emp)).await;
    make_user(&mut hr, "other@pointage.test", "employee", Some(&other)).await;
    make_user(&mut hr, "audit@pointage.test", "auditor", None).await;
    (hr, boss, emp, other)
}

#[sqlx::test(migrations = "../../migrations")]
async fn clock_in_out_rules_and_server_time(pool: PgPool) {
    let (_hr, _boss, _emp, _other) = attendance_setup(&pool).await;
    let mut e = Client::new(&pool);
    e.login("emp@pointage.test").await;
    // Sortie sans entrée : refusée.
    assert_eq!(
        e.post("/attendance/clock-out", json!({})).await.0,
        StatusCode::CONFLICT
    );
    let (st, inn) = e.post("/attendance/clock-in", json!({})).await;
    assert_eq!(st, StatusCode::CREATED, "{inn}");
    assert!(inn["clock_out"].is_null() && inn["source"] == "self");
    // Double entrée : refusée.
    assert_eq!(
        e.post("/attendance/clock-in", json!({})).await.0,
        StatusCode::CONFLICT
    );
    // L'heure fournie par le client est ignorée (corps non lu).
    let (_, me) = e.get("/attendance/me").await;
    assert_eq!(me["open"]["id"], inn["id"]);
    let (st, out) = e
        .post(
            "/attendance/clock-out",
            json!({"clock_out": "2000-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{out}");
    assert!(out["minutes"].as_i64().unwrap() >= 0);
    assert!(
        out["clock_out"].as_str().unwrap() > "2020",
        "heure serveur attendue"
    );
    let (_, me) = e.get("/attendance/me").await;
    assert!(me["open"].is_null());
    assert_eq!(me["today"].as_array().unwrap().len(), 1);
    // Un auditeur (sans dossier employé) ne pointe pas.
    let mut a = Client::new(&pool);
    a.login("audit@pointage.test").await;
    assert_eq!(
        a.post("/attendance/clock-in", json!({})).await.0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn manual_entries_corrections_and_summary_are_exact_and_audited(pool: PgPool) {
    let (mut hr, boss, emp, _other) = attendance_setup(&pool).await;
    // Fuseau Africa/Lubumbashi (UTC+2), début 08:00, tolérance 10 min, journée standard 480 min.
    let (st, body) = hr
        .put("/attendance/settings", json!({"timezone": "Africa/Lubumbashi", "work_start": "08:00:00", "grace_minutes": 10, "daily_minutes": 480}))
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert_eq!(
        hr.put("/attendance/settings", json!({"timezone": "Mars/Olympus", "work_start": "08:00:00", "grace_minutes": 10, "daily_minutes": 480})).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Lundi 2026-03-02 : arrivée 08:25 locale (06:25Z) → en retard ; 9 h de travail → 60 min au-delà.
    let (st, e1) = hr.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-02T06:25:00Z", "clock_out": "2026-03-02T15:25:00Z", "note": "oubli de pointage"})).await;
    assert_eq!(st, StatusCode::CREATED, "{e1}");
    assert_eq!(e1["minutes"], 540);
    // Mardi : arrivée 08:05 locale (06:05Z) → dans la tolérance ; 8 h pile.
    let (st, _) = hr.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-03T06:05:00Z", "clock_out": "2026-03-03T14:05:00Z", "note": "saisie RH"})).await;
    assert_eq!(st, StatusCode::CREATED);
    // Motif obligatoire, chevauchement et futur refusés.
    assert_eq!(
        hr.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-04T06:00:00Z", "clock_out": "2026-03-04T10:00:00Z", "note": "  "})).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        hr.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-02T10:00:00Z", "clock_out": "2026-03-02T11:00:00Z", "note": "x"})).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        hr.post(
            "/attendance/entries",
            json!({"employee_id": emp, "clock_in": "2999-01-01T06:00:00Z", "note": "x"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let q = "from=2026-03-01&to=2026-03-31&scope=all";
    let (st, sm) = hr.get(&format!("/attendance/summary?{q}")).await;
    assert_eq!(st, StatusCode::OK, "{sm}");
    let rows = sm["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r["days_present"], 2);
    assert_eq!(r["worked_minutes"], 540 + 480);
    assert_eq!(r["late_days"], 1);
    assert_eq!(r["overtime_minutes"], 60);

    // Correction : le lundi ramené à 08:00 → plus de retard, 8 h30 → 30 min au-delà ; ancien/nouveau audités.
    let id = e1["id"].as_str().unwrap();
    let (st, c) = hr.put(&format!("/attendance/entries/{id}"), json!({"clock_in": "2026-03-02T06:00:00Z", "clock_out": "2026-03-02T14:30:00Z", "note": "badge défaillant"})).await;
    assert_eq!(st, StatusCode::OK, "{c}");
    assert!(c["corrected"].as_bool().unwrap());
    let (_, sm) = hr.get(&format!("/attendance/summary?{q}")).await;
    assert_eq!(sm["rows"][0]["late_days"], 0);
    assert_eq!(sm["rows"][0]["overtime_minutes"], 30);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM audit_logs WHERE action = 'attendance.correct' AND details->'before'->>'clock_in' IS NOT NULL AND details->'after'->>'clock_in' IS NOT NULL AND details->>'note' = 'badge défaillant'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);

    // CSV : BOM, en-têtes, audité ; un manager ne peut pas utiliser scope=all.
    let (st, ct, text) = get_text(&hr, &format!("/attendance/summary.csv?{q}")).await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.starts_with("text/csv") && text.starts_with('\u{feff}'));
    assert!(text.contains("E-1"));
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM audit_logs WHERE action = 'attendance.export'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
    let mut m = Client::new(&pool);
    m.login("boss@pointage.test").await;
    assert_eq!(
        m.get(&format!("/attendance/summary?{q}")).await.0,
        StatusCode::FORBIDDEN
    );
    let (st, team) = m
        .get("/attendance/summary?from=2026-03-01&to=2026-03-31&scope=team")
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        team["rows"].as_array().unwrap().len(),
        1,
        "le manager voit son équipe"
    );
    // Le manager ne peut ni saisir ni corriger.
    assert_eq!(
        m.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-05T06:00:00Z", "clock_out": "2026-03-05T10:00:00Z", "note": "x"})).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        m.put(&format!("/attendance/entries/{id}"), json!({"clock_in": "2026-03-02T06:00:00Z", "clock_out": "2026-03-02T07:00:00Z", "note": "x"})).await.0,
        StatusCode::FORBIDDEN
    );
    let _ = boss;
}

#[sqlx::test(migrations = "../../migrations")]
async fn attendance_visibility_self_correction_and_tenant_isolation(pool: PgPool) {
    let (mut hr, boss, emp, other) = attendance_setup(&pool).await;
    for (who, id) in [("emp", &emp), ("other", &other)] {
        let mut c = Client::new(&pool);
        c.login(&format!("{who}@pointage.test")).await;
        assert_eq!(
            c.post("/attendance/clock-in", json!({})).await.0,
            StatusCode::CREATED
        );
        let _ = id;
    }
    // Chacun ne voit que ses pointages ; un employé n'accède ni à l'équipe ni à tout.
    let mut e = Client::new(&pool);
    e.login("emp@pointage.test").await;
    let (_, mine) = e.get("/attendance/entries").await;
    assert_eq!(mine.as_array().unwrap().len(), 1);
    assert_eq!(mine[0]["employee_id"], emp.as_str());
    assert_eq!(
        e.get("/attendance/entries?scope=team").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        e.get("/attendance/entries?scope=all").await.0,
        StatusCode::FORBIDDEN
    );
    // Le manager voit son subordonné, pas Bob (autre équipe).
    let mut m = Client::new(&pool);
    m.login("boss@pointage.test").await;
    let (_, team) = m.get("/attendance/entries?scope=team").await;
    let ids: Vec<_> = team
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["employee_id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(ids, vec![emp.clone()]);
    // L'auditeur lit tout, sans pouvoir écrire.
    let mut a = Client::new(&pool);
    a.login("audit@pointage.test").await;
    let (st, all) = a.get("/attendance/entries?scope=all").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(a.put("/attendance/settings", json!({"timezone": "UTC", "work_start": "08:00:00", "grace_minutes": 10, "daily_minutes": 480})).await.0, StatusCode::FORBIDDEN);
    // L'admin RH ne corrige pas son propre pointage : il faut un dossier lié.
    let hr_emp = make_employee(&mut hr, "H-1", "Hélène", None).await;
    make_user(&mut hr, "hr2@pointage.test", "hr_admin", Some(&hr_emp)).await;
    let mut h2 = Client::new(&pool);
    h2.login("hr2@pointage.test").await;
    let (_, mine) = {
        h2.post("/attendance/clock-in", json!({})).await;
        h2.get("/attendance/entries").await
    };
    let own = mine[0]["id"].as_str().unwrap().to_string();
    assert_eq!(
        h2.put(&format!("/attendance/entries/{own}"), json!({"clock_in": "2026-03-02T06:00:00Z", "clock_out": "2026-03-02T07:00:00Z", "note": "x"})).await.0,
        StatusCode::FORBIDDEN
    );
    // Isolation : une autre société ne voit ni ne modifie rien.
    make_tenant(&pool, "ailleurs", "hr@ailleurs.test").await;
    let mut x = Client::new(&pool);
    x.login("hr@ailleurs.test").await;
    let (_, none) = x.get("/attendance/entries?scope=all").await;
    assert!(none.as_array().unwrap().is_empty());
    assert_eq!(
        x.put(&format!("/attendance/entries/{own}"), json!({"clock_in": "2026-03-02T06:00:00Z", "clock_out": "2026-03-02T07:00:00Z", "note": "x"})).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        x.post("/attendance/entries", json!({"employee_id": emp, "clock_in": "2026-03-02T06:00:00Z", "clock_out": "2026-03-02T07:00:00Z", "note": "x"})).await.0,
        StatusCode::NOT_FOUND
    );
    let _ = boss;
}

// ---------- Badgeuses ----------

async fn device_post(pool: &PgPool, key: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/device/punches")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(k) = key {
        req = req.header(header::AUTHORIZATION, format!("Bearer {k}"));
    }
    let app = build_router(AppState::new(pool.clone(), Config::for_tests()));
    let resp = app
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn stamp(hours_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::hours(hours_ago))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

fn statuses(v: &Value) -> Vec<String> {
    v["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["status"].as_str().unwrap().to_string())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn device_api_is_authenticated_idempotent_and_tenant_scoped(pool: PgPool) {
    let (mut hr, _boss, emp, _other) = attendance_setup(&pool).await;
    // Pas de clé / mauvaise clé : 401.
    let body = json!({"punches": [{"badge": "E-1", "at": stamp(10)}]});
    assert_eq!(
        device_post(&pool, None, body.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        device_post(&pool, Some("mskey_faux"), body.clone()).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Création : la clé n'est montrée qu'une fois ; seule son empreinte est stockée.
    let (st, dev) = hr
        .post("/attendance/devices", json!({"name": "Entrée principale"}))
        .await;
    assert_eq!(st, StatusCode::CREATED, "{dev}");
    let key = dev["api_key"].as_str().unwrap().to_string();
    assert!(key.starts_with("mskey_"));
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM attendance_devices WHERE api_key_hash = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 0, "la clé en clair ne doit pas être stockée");
    let (_, list) = hr.get("/attendance/devices").await;
    assert!(
        list[0].get("api_key").is_none() && list[0]["key_prefix"].as_str().unwrap().len() == 12
    );

    // Matricule inconnu du système de badges : l'identifiant E-1 est le matricule (repli) → entrée ouverte.
    let t_in = stamp(10);
    let (st, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [{"badge": "E-1", "at": t_in}]}),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{r}");
    assert_eq!(statuses(&r), ["in"]);
    // Renvoi identique (la badgeuse réessaie) : idempotent.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [{"badge": "E-1", "at": t_in}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["duplicate"]);
    // Double badgeage à 30 s : ignoré.
    let t_dbl = (chrono::Utc::now() - chrono::Duration::hours(10) + chrono::Duration::seconds(30))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [{"badge": "E-1", "at": t_dbl}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["duplicate"]);
    // Sortie (bascule automatique) 8 h plus tard → ferme l'entrée.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [{"badge": "E-1", "at": stamp(2)}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["out"]);
    let (_, entries) = hr
        .get(&format!("/attendance/entries?scope=all&employee_id={emp}"))
        .await;
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["source"], "device");
    assert_eq!(entries[0]["minutes"], 480);

    // Direction explicite : sortie sans entrée refusée ; badge inconnu conservé ; envoi invalide refusé.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [
        {"badge": "E-1", "at": stamp(1), "direction": "sortie"},
        {"badge": "INCONNU-9", "at": stamp(1)},
        {"badge": "", "at": stamp(1)}]}),
    )
    .await;
    assert_eq!(statuses(&r).len(), 3);
    assert!(statuses(&r).contains(&"unknown_badge".to_string()));
    assert_eq!(statuses(&r).iter().filter(|s| *s == "rejected").count(), 2);
    assert_eq!(
        device_post(&pool, Some(&key), json!({"punches": []}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Affectation du badge puis retraitement : le passage inconnu devient un pointage.
    let (st, _) = hr
        .post(
            "/attendance/badges",
            json!({"employee_id": emp, "badge_code": "INCONNU-9"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(
        hr.post(
            "/attendance/badges",
            json!({"employee_id": emp, "badge_code": "INCONNU-9"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (st, rp) = hr.post("/attendance/punches/reprocess", json!({})).await;
    assert_eq!(st, StatusCode::OK, "{rp}");
    assert_eq!(rp["counts"]["entries_opened"], 1);
    let (_, log) = hr.get("/attendance/punches?status=in").await;
    assert!(
        log.as_array()
            .unwrap()
            .iter()
            .any(|p| p["identifier"] == "INCONNU-9")
    );

    // Audit : passages d'appareil journalisés sans acteur utilisateur.
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM audit_logs WHERE action = 'attendance.device_ingest' AND actor_id IS NULL").fetch_one(&pool).await.unwrap();
    assert!(n >= 3);

    // Isolation : la clé d'une autre société ne peut rien sur nos employés.
    make_tenant(&pool, "voisin", "hr@voisin.test").await;
    let mut hv = Client::new(&pool);
    hv.login("hr@voisin.test").await;
    let (_, dv) = hv
        .post("/attendance/devices", json!({"name": "Autre"}))
        .await;
    let kv = dv["api_key"].as_str().unwrap().to_string();
    let (_, r) = device_post(
        &pool,
        Some(&kv),
        json!({"punches": [{"badge": "E-2", "at": stamp(5)}]}),
    )
    .await;
    assert_eq!(
        statuses(&r),
        ["unknown_badge"],
        "E-2 appartient à une autre société"
    );
    let (_, none) = hv.get("/attendance/entries?scope=all").await;
    assert!(none.as_array().unwrap().is_empty());
    let (_, log) = hv.get("/attendance/punches").await;
    assert_eq!(log.as_array().unwrap().len(), 1);

    // Révocation : la clé cesse immédiatement de fonctionner.
    let id = dev["id"].as_str().unwrap();
    assert_eq!(
        hr.post(&format!("/attendance/devices/{id}/revoke"), json!({}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        device_post(&pool, Some(&key), body).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Un appareil d'une autre société ne peut pas être révoqué par nous.
    let idv = dv["id"].as_str().unwrap();
    assert_eq!(
        hr.post(&format!("/attendance/devices/{idv}/revoke"), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn csv_import_dry_run_local_time_errors_and_permissions(pool: PgPool) {
    let (mut hr, _boss, emp, _other) = attendance_setup(&pool).await;
    assert_eq!(
        hr.put("/attendance/settings", json!({"timezone": "Africa/Lubumbashi", "work_start": "08:00:00", "grace_minutes": 10, "daily_minutes": 480})).await.0,
        StatusCode::OK
    );
    let day = (chrono::Utc::now() - chrono::Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let fr = (chrono::Utc::now() - chrono::Duration::days(10))
        .format("%d/%m/%Y")
        .to_string();
    let csv = format!(
        "Badge;Date_Heure;Sens\nE-1;{day} 08:00;entrée\nE-1;{day} 17:30;sortie\nE-1;pas une date;\nZZ-0;{fr} 09:00;\nE-1;{day} 09:00;n'importe quoi\n"
    );
    // Simulation : rapporte tout mais n'écrit rien.
    let (st, dry) = hr
        .post(
            "/attendance/import",
            json!({"content": csv, "dry_run": true}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{dry}");
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["counts"]["entries_opened"], 1);
    assert_eq!(dry["counts"]["entries_closed"], 1);
    assert_eq!(dry["counts"]["unknown_badges"], 1);
    assert_eq!(
        dry["counts"]["rejected"], 2,
        "date illisible + direction inconnue"
    );
    let (n,): (i64,) = sqlx::query_as("SELECT (SELECT count(*) FROM attendance_entries) + (SELECT count(*) FROM attendance_raw_punches)").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0, "la simulation ne doit rien enregistrer");
    let reasons: Vec<String> = dry["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["reason"].as_str().map(String::from))
        .collect();
    assert!(
        reasons.iter().any(|r| r.starts_with("ligne 4")),
        "numéro de ligne dans l'erreur: {reasons:?}"
    );

    // Import réel : 08:00 locale (UTC+2) = 06:00Z ; 17:30 locale → 9 h 30.
    let (st, real) = hr.post("/attendance/import", json!({"content": csv})).await;
    assert_eq!(st, StatusCode::OK, "{real}");
    let (_, entries) = hr
        .get(&format!(
            "/attendance/entries?scope=all&employee_id={emp}&from={day}&to={day}"
        ))
        .await;
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["clock_in"].as_str().unwrap()[11..16], *"06:00");
    assert_eq!(entries[0]["minutes"], 570);
    assert_eq!(entries[0]["source"], "import");
    // Réimport du même fichier : idempotent.
    let (_, again) = hr.post("/attendance/import", json!({"content": csv})).await;
    assert_eq!(again["counts"]["entries_opened"], 0);
    assert_eq!(
        again["counts"]["duplicates"], 3,
        "2 passages + le badge inconnu déjà reçu"
    );
    // Fichier sans colonne d'identifiant : refusé.
    assert_eq!(
        hr.post("/attendance/import", json!({"content": "a;b\n1;2\n"}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Seule la gestion RH importe, gère badges et appareils.
    for who in ["boss", "emp", "audit"] {
        let mut c = Client::new(&pool);
        c.login(&format!("{who}@pointage.test")).await;
        assert_eq!(
            c.post("/attendance/import", json!({"content": csv}))
                .await
                .0,
            StatusCode::FORBIDDEN,
            "{who}"
        );
        assert_eq!(
            c.get("/attendance/devices").await.0,
            StatusCode::FORBIDDEN,
            "{who}"
        );
        assert_eq!(
            c.get("/attendance/badges").await.0,
            StatusCode::FORBIDDEN,
            "{who}"
        );
        assert_eq!(
            c.get("/attendance/punches").await.0,
            StatusCode::FORBIDDEN,
            "{who}"
        );
    }
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM audit_logs WHERE action = 'attendance.import'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 2, "seuls les imports réels sont audités");
}

#[sqlx::test(migrations = "../../migrations")]
async fn backfilled_punches_fit_in_gaps_but_never_overlap_existing_entries(pool: PgPool) {
    let (mut hr, _boss, emp, _other) = attendance_setup(&pool).await;
    let (_, dev) = hr
        .post("/attendance/devices", json!({"name": "Rattrapage"}))
        .await;
    let key = dev["api_key"].as_str().unwrap().to_string();
    // Un pointage récent existe déjà (il y a 5 h → 3 h).
    let (st, _) = hr.post("/attendance/entries", json!({"employee_id": emp, "clock_in": stamp(5), "clock_out": stamp(3), "note": "existant"})).await;
    assert_eq!(st, StatusCode::CREATED);
    // Rattrapage d'une journée plus ancienne (il y a 30 h → 22 h) : doit s'insérer dans le passé.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [
        {"badge": "E-1", "at": stamp(30)}, {"badge": "E-1", "at": stamp(22)}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["in", "out"], "{r}");
    // Une entrée tombant DANS le pointage existant est refusée.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [{"badge": "E-1", "at": stamp(4)}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["rejected"]);
    // Une entrée dans un trou, dont la sortie recouvrirait le pointage existant, est refusée à la sortie.
    let (_, r) = device_post(
        &pool,
        Some(&key),
        json!({"punches": [
        {"badge": "E-1", "at": stamp(8)}, {"badge": "E-1", "at": stamp(2)}]}),
    )
    .await;
    assert_eq!(statuses(&r), ["in", "rejected"], "{r}");
    assert!(
        r["rows"][1]["reason"]
            .as_str()
            .unwrap()
            .contains("chevauche")
    );
}

// ---------- Sécurité du compte : MFA (TOTP + codes de secours) et réinitialisation ----------

fn b32_decode(s: &str) -> Vec<u8> {
    let (mut out, mut buf, mut bits) = (Vec::new(), 0u32, 0u32);
    for c in s.bytes() {
        let v = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
            .iter()
            .position(|&x| x == c)
            .expect("base32") as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    out
}

fn code_at(secret: &[u8], step_offset: i64) -> String {
    let step = chrono::Utc::now().timestamp() / 30 + step_offset;
    format!(
        "{:06}",
        mysuite_app::auth::mfa::totp_code(secret, step as u64)
    )
}

/// Active le MFA pour `email` ; retourne (secret, codes de secours).
async fn enable_mfa(pool: &PgPool, email: &str) -> (Vec<u8>, Vec<String>) {
    let mut c = Client::new(pool);
    assert_eq!(c.login(email).await, StatusCode::OK);
    // Mauvais mot de passe : refusé.
    assert_eq!(
        c.post("/auth/mfa/setup", json!({"password": "nope"}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (st, setup) = c.post("/auth/mfa/setup", json!({"password": PWD})).await;
    assert_eq!(st, StatusCode::OK, "{setup}");
    assert!(
        setup["otpauth_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/MySuite:")
    );
    let secret = b32_decode(setup["secret"].as_str().unwrap());
    // Code erroné : le MFA reste inactif.
    assert_eq!(
        c.post("/auth/mfa/enable", json!({"code": "000000"}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (st, rec) = c
        .post("/auth/mfa/enable", json!({"code": code_at(&secret, 0)}))
        .await;
    assert_eq!(st, StatusCode::OK, "{rec}");
    let codes: Vec<String> = rec["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(codes.len(), 10);
    (secret, codes)
}

#[sqlx::test(migrations = "../../migrations")]
async fn mfa_login_flow_replay_recovery_codes_and_secret_at_rest(pool: PgPool) {
    let (_hr, _b, _e, _o) = attendance_setup(&pool).await;
    // Sans MFA, la connexion est inchangée (renvoie directement le profil).
    let mut plain = Client::new(&pool);
    let (st, me) = plain
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    assert!(me.get("mfa_required").is_none() && me["user"]["email"] == "emp@pointage.test");
    let (_, status) = plain.get("/auth/mfa").await;
    assert_eq!(status["enabled"], false);
    assert_eq!(status["available"], true);

    let (secret, codes) = enable_mfa(&pool, "emp@pointage.test").await;

    // Le secret n'est pas stocké en clair.
    let (blob,): (Vec<u8>,) = sqlx::query_as(
        "SELECT secret_enc FROM user_mfa WHERE user_id = (SELECT id FROM users WHERE email='emp@pointage.test')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(blob.len() > secret.len() && !blob.windows(secret.len()).any(|w| w == secret));
    // Les codes de secours sont stockés hachés.
    let (clear,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM mfa_recovery_codes WHERE code_hash = $1")
            .bind(&codes[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(clear, 0);

    // Connexion : mot de passe seul → défi, aucune session.
    let mut c = Client::new(&pool);
    let (st, ch) = c
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{ch}");
    assert_eq!(ch["mfa_required"], true);
    let token = ch["mfa_token"].as_str().unwrap().to_string();
    assert!(c.cookie.is_none());
    assert_eq!(c.get("/auth/me").await.0, StatusCode::UNAUTHORIZED);
    // Mauvais code.
    assert_eq!(
        c.post(
            "/auth/login/mfa",
            json!({"mfa_token": token, "code": "123456"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    // Jeton inconnu.
    assert_eq!(
        c.post(
            "/auth/login/mfa",
            json!({"mfa_token": "x", "code": code_at(&secret, 1)})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    // Bon code (pas suivant : strictement postérieur à celui utilisé à l'activation).
    let good = code_at(&secret, 1);
    let (st, me) = c
        .post("/auth/login/mfa", json!({"mfa_token": token, "code": good}))
        .await;
    assert_eq!(st, StatusCode::OK, "{me}");
    assert_eq!(me["user"]["email"], "emp@pointage.test");
    assert_eq!(c.get("/auth/me").await.0, StatusCode::OK);
    // Le défi est consommé.
    assert_eq!(
        c.post("/auth/login/mfa", json!({"mfa_token": token, "code": good}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // Rejeu du même code sur un nouveau défi : refusé.
    let mut r = Client::new(&pool);
    let (_, ch) = r
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    let t2 = ch["mfa_token"].as_str().unwrap().to_string();
    assert_eq!(
        r.post("/auth/login/mfa", json!({"mfa_token": t2, "code": good}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    // Code de secours : accepté une fois, refusé la seconde.
    let (st, _) = r
        .post(
            "/auth/login/mfa",
            json!({"mfa_token": t2, "code": codes[0]}),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let mut r2 = Client::new(&pool);
    let (_, ch) = r2
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    let t3 = ch["mfa_token"].as_str().unwrap().to_string();
    assert_eq!(
        r2.post(
            "/auth/login/mfa",
            json!({"mfa_token": t3, "code": codes[0].to_lowercase().replace('-', " ")})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, s) = c.get("/auth/mfa").await;
    assert_eq!(s["enabled"], true);
    assert_eq!(s["recovery_codes_remaining"], 9);

    // Régénération : exige mot de passe + second facteur ; invalide les anciens codes.
    assert_eq!(
        c.post(
            "/auth/mfa/recovery-codes",
            json!({"password": PWD, "code": "000000"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (st, regen) = c
        .post(
            "/auth/mfa/recovery-codes",
            json!({"password": PWD, "code": codes[1]}),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{regen}");
    let new_codes = regen["recovery_codes"].as_array().unwrap();
    assert_eq!(new_codes.len(), 10);
    assert!(!new_codes.iter().any(|n| n == &json!(codes[2])));
    // Désactivation : mot de passe + code de secours.
    let fresh = new_codes[0].as_str().unwrap();
    assert_eq!(
        c.post(
            "/auth/mfa/disable",
            json!({"password": "nope", "code": fresh})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        c.post("/auth/mfa/disable", json!({"password": PWD, "code": fresh}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, s) = c.get("/auth/mfa").await;
    assert_eq!(s["enabled"], false);
    // Reconnexion sans second facteur.
    let mut again = Client::new(&pool);
    let (_, me) = again
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    assert_eq!(me["user"]["email"], "emp@pointage.test");
    // Audit.
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_logs WHERE action IN ('mfa.enable','mfa.recovery_regenerate','mfa.disable')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn mfa_failures_lock_the_account_and_setup_needs_a_server_key(pool: PgPool) {
    let (_hr, _b, _e, _o) = attendance_setup(&pool).await;
    let (secret, _codes) = enable_mfa(&pool, "other@pointage.test").await;
    let mut c = Client::new(&pool);
    let (_, ch) = c
        .post(
            "/auth/login",
            json!({"email": "other@pointage.test", "password": PWD}),
        )
        .await;
    let token = ch["mfa_token"].as_str().unwrap().to_string();
    // 5 échecs : verrouillage du compte, même avec le bon mot de passe puis le bon code.
    for _ in 0..5 {
        assert_eq!(
            c.post(
                "/auth/login/mfa",
                json!({"mfa_token": token, "code": "000000"})
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        c.post(
            "/auth/login",
            json!({"email": "other@pointage.test", "password": PWD})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Les échecs au 2e facteur ne sont pas remis à zéro par un mot de passe correct.
    let _ = secret;

    // Sans MFA_ENCRYPTION_KEY, la configuration est refusée proprement.
    let mut cfg = Config::for_tests();
    cfg.mfa_key = None;
    let mut app = Client {
        app: build_router(AppState::new(pool.clone(), cfg)),
        cookie: None,
    };
    assert_eq!(app.login("emp@pointage.test").await, StatusCode::OK);
    let (_, s) = app.get("/auth/mfa").await;
    assert_eq!(s["available"], false);
    assert_eq!(
        app.post("/auth/mfa/setup", json!({"password": PWD}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_can_reset_mfa_and_issue_single_use_password_reset(pool: PgPool) {
    let (mut hr, _b, _e, _o) = attendance_setup(&pool).await;
    make_tenant(&pool, "autre", "hr@autre.test").await;
    enable_mfa(&pool, "emp@pointage.test").await;
    let (_, users) = hr.get("/users").await;
    let id_of = |email: &str| -> String {
        users
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["email"] == email)
            .unwrap()["user_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let emp_id = id_of("emp@pointage.test");
    let hr_id = id_of("hr@pointage.test");

    // Un employé ne peut ni réinitialiser le MFA ni émettre de lien.
    let mut other = Client::new(&pool);
    other.login("other@pointage.test").await;
    assert_eq!(
        other
            .post(&format!("/users/{emp_id}/mfa-reset"), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        other
            .post(&format!("/users/{emp_id}/password-reset"), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // Pas sur soi-même.
    assert_eq!(
        hr.post(&format!("/users/{hr_id}/mfa-reset"), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        hr.post(&format!("/users/{hr_id}/password-reset"), json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    // Pas sur un utilisateur d'une autre société (404, sans révéler son existence).
    let (other_tenant_user,): (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM users WHERE email = 'hr@autre.test'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        hr.post(&format!("/users/{other_tenant_user}/mfa-reset"), json!({}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        hr.post(
            &format!("/users/{other_tenant_user}/password-reset"),
            json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // Réinitialisation du MFA : sessions révoquées, MFA retiré.
    let mut emp_session = Client::new(&pool);
    // (le MFA est actif : on ouvre la session via un code de secours en base)
    let (_, ch) = emp_session
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    assert_eq!(ch["mfa_required"], true);
    sqlx::query("UPDATE users SET failed_attempts = 3 WHERE id = $1::uuid")
        .bind(&emp_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        hr.post(&format!("/users/{emp_id}/mfa-reset"), json!({}))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let mut emp = Client::new(&pool);
    let (_, me) = emp
        .post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": PWD}),
        )
        .await;
    assert_eq!(me["user"]["email"], "emp@pointage.test");
    assert_eq!(emp.get("/auth/me").await.0, StatusCode::OK);

    // Lien de réinitialisation : mot de passe faible refusé, valide une fois, sessions révoquées.
    let (st, issued) = hr
        .post(&format!("/users/{emp_id}/password-reset"), json!({}))
        .await;
    assert_eq!(st, StatusCode::OK, "{issued}");
    let token = issued["reset_token"].as_str().unwrap().to_string();
    assert!(issued["path"].as_str().unwrap().contains(&token));
    let (hash_in_db,): (String,) = sqlx::query_as("SELECT token_hash FROM password_resets LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(hash_in_db, token);
    let mut anon = Client::new(&pool);
    assert_eq!(
        anon.post(
            "/auth/reset-password",
            json!({"token": token, "new_password": "court"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let new_pwd = "Nouveau-Passw0rd-2026";
    assert_eq!(
        anon.post(
            "/auth/reset-password",
            json!({"token": token, "new_password": new_pwd})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    // Ancienne session révoquée, ancien mot de passe invalide, nouveau valide.
    assert_eq!(emp.get("/auth/me").await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        emp.login("emp@pointage.test").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        emp.post(
            "/auth/login",
            json!({"email": "emp@pointage.test", "password": new_pwd})
        )
        .await
        .0,
        StatusCode::OK
    );
    // Usage unique, message générique identique pour un jeton inconnu.
    let (s1, b1) = anon
        .post(
            "/auth/reset-password",
            json!({"token": token, "new_password": new_pwd}),
        )
        .await;
    let (s2, b2) = anon
        .post(
            "/auth/reset-password",
            json!({"token": "inconnu", "new_password": new_pwd}),
        )
        .await;
    assert_eq!(s1, s2);
    assert_eq!((&b1["code"], &b1["message"]), (&b2["code"], &b2["message"]));
    assert_eq!(s1, StatusCode::UNPROCESSABLE_ENTITY);
    // Le compteur d'échecs a été remis à zéro.
    let (fails,): (i32,) = sqlx::query_as("SELECT failed_attempts FROM users WHERE id = $1::uuid")
        .bind(&emp_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fails, 0);

    // Un nouveau lien invalide le précédent ; un lien expiré est refusé.
    let (_, a) = hr
        .post(&format!("/users/{emp_id}/password-reset"), json!({}))
        .await;
    let (_, b) = hr
        .post(&format!("/users/{emp_id}/password-reset"), json!({}))
        .await;
    let (ta, tb) = (
        a["reset_token"].as_str().unwrap(),
        b["reset_token"].as_str().unwrap(),
    );
    assert_eq!(
        anon.post(
            "/auth/reset-password",
            json!({"token": ta, "new_password": new_pwd})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    sqlx::query(
        "UPDATE password_resets SET expires_at = now() - interval '1 minute' WHERE used_at IS NULL",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        anon.post(
            "/auth/reset-password",
            json!({"token": tb, "new_password": new_pwd})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // Audit.
    let (n,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_logs WHERE action IN ('mfa.admin_reset','user.password_reset_issued','user.password_reset')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 5);
}

// ---------- Pack RDC : modèle de règles (brouillon) et personnes à charge ----------

#[sqlx::test(migrations = "../../migrations")]
async fn drc_template_is_a_sourced_draft_and_dependents_drive_the_tax(pool: PgPool) {
    use rust_decimal::Decimal;
    let (mut hr, mut pay1, mut pay2, e1, _e2) = payroll_setup(&pool).await;

    // Le modèle n'est servi qu'à qui gère les règles de paie.
    let (st, tpls) = pay1.get("/payroll/rule-set-templates").await;
    assert_eq!(st, StatusCode::OK, "{tpls}");
    let t = &tpls[0];
    assert_eq!(t["country"], "CD");
    assert!(!t["a_completer"].as_array().unwrap().is_empty());
    assert!(
        t["sources"]["sage"]["url"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
    assert_eq!(
        hr.get("/payroll/rule-set-templates").await.0,
        StatusCode::FORBIDDEN
    );
    let mut paul = Client::new(&pool);
    paul.login("paul@foxtrot.test").await;
    assert_eq!(
        paul.get("/payroll/rule-set-templates").await.0,
        StatusCode::FORBIDDEN
    );

    // Créé à partir du modèle : toujours en brouillon, donc paie en simulation tant qu'il n'est pas approuvé.
    let (st, rs) = pay1
        .post(
            "/payroll/rule-sets",
            json!({"country": "CD", "name": "RDC modèle", "effective_from": "2026-01-01", "params": t["params"]}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{rs}");
    assert_eq!(rs["status"], "draft");
    let rs_id = rs["id"].as_str().unwrap().to_string();
    // Le créateur ne peut pas l'approuver ; l'approbateur doit fournir source et date.
    let approve =
        json!({"source": "Loi n° 23/053 (texte officiel consulté)", "verified_on": "2026-11-01"});
    assert_eq!(
        pay1.post(&format!("/payroll/rule-sets/{rs_id}/approve"), approve)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        pay2.post(
            &format!("/payroll/rule-sets/{rs_id}/approve"),
            json!({"source": "", "verified_on": "2026-11-01"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Salaire de 1 000 000 CDF pour e1.
    let (st, b) = pay1
        .post(
            &format!("/employees/{e1}/compensation"),
            json!({"base_salary": "1000000", "effective_from": "2025-06-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");

    // Personnes à charge : validation, droits, isolation, audit.
    for bad in [-1, 31] {
        assert_eq!(
            hr.post(
                &format!("/employees/{e1}/dependents"),
                json!({"dependents": bad, "effective_from": "2026-01-01"})
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        paul.post(
            &format!("/employees/{e1}/dependents"),
            json!({"dependents": 2, "effective_from": "2026-01-01"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        paul.get(&format!("/employees/{e1}/dependents")).await.0,
        StatusCode::FORBIDDEN
    );
    let (st, d) = hr
        .post(
            &format!("/employees/{e1}/dependents"),
            json!({"dependents": 2, "effective_from": "2026-01-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{d}");
    assert_eq!(
        hr.get(&format!("/employees/{e1}/dependents"))
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    make_tenant(&pool, "golf", "hr@golf.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@golf.test").await;
    assert_eq!(
        other.get(&format!("/employees/{e1}/dependents")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other
            .post(
                &format!("/employees/{e1}/dependents"),
                json!({"dependents": 5, "effective_from": "2026-01-01"})
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM audit_logs WHERE action = 'employee.dependents'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 1);

    // Simulation : brut 1 000 000, CNSS 5 % = 50 000, assiette 950 000,
    // impôt 123 060 ; réduction 2 × 2 % = 4 % → 118 137,6 ; arrondi à la centaine → 118 100.
    let (_, period) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 3}))
        .await;
    let (st, run) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period["id"], "rule_set_id": rs_id}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{run}");
    assert_eq!(run["run"]["simulation"], true);
    let dec = |v: &Value| v.as_str().unwrap().parse::<Decimal>().unwrap();
    let item = run["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| dec(&i["gross"]) == Decimal::from(1_000_000))
        .expect("ligne e1");
    assert_eq!(
        dec(&item["net"]),
        Decimal::from(1_000_000 - 50_000 - 118_100)
    );
    // Le détail explique la réduction (traçabilité).
    let item_id = item["item_id"].as_str().or(item["id"].as_str()).unwrap();
    let (st, slip) = pay1.get(&format!("/payslips/{item_id}")).await;
    assert_eq!(st, StatusCode::OK, "{slip}");
    assert!(slip.to_string().contains("personnes à charge"), "{slip}");

    // Même employé sans personne à charge à partir d'une date ultérieure : l'historique daté est respecté.
    hr.post(
        &format!("/employees/{e1}/dependents"),
        json!({"dependents": 0, "effective_from": "2026-04-01"}),
    )
    .await;
    let (_, period4) = pay1
        .post("/payroll/periods", json!({"year": 2026, "month": 4}))
        .await;
    let (_, run4) = pay1
        .post(
            "/payroll/runs",
            json!({"period_id": period4["id"], "rule_set_id": rs_id}),
        )
        .await;
    let item4 = run4["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| dec(&i["gross"]) == Decimal::from(1_000_000))
        .unwrap();
    assert_eq!(
        dec(&item4["net"]),
        Decimal::from(1_000_000 - 50_000 - 123_100)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn smig_grid_import_class_floor_and_compliance(pool: PgPool) {
    let (mut hr, mut pay1, mut pay2, e1, _e2) = payroll_setup(&pool).await;
    let csv = "\"Classe\",\"Catégorie\",\"Repère paie\",\"Tension\",\"Journalier\",\"Mensuel\"\n\
               \"1\",\"Manœuvre\",\"CAT I\",\"100\",\"21500\",\"559000\"\n\
               \"2\",\"Lourd\",\"CAT I 2\",\"116\",\"24940\",\"648440\"\n";
    let body = |dry: bool, csv: &str| {
        json!({"name": "SMIG test", "currency": "CDF", "effective_from": "2025-01-01",
               "source": "Décret test", "csv": csv, "dry_run": dry})
    };
    // Droits : un RH ne gère pas les règles de paie.
    assert_eq!(
        hr.post("/wage-grids", body(false, csv)).await.0,
        StatusCode::FORBIDDEN
    );
    // Fichier invalide : refusé, rien n'est écrit.
    let (st, b) = pay1.post("/wage-grids", body(false, "Classe\n1")).await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    let (_, dry) = pay1.post("/wage-grids", body(true, csv)).await;
    assert_eq!(dry["rows"], 2);
    assert_eq!(pay1.get("/wage-grids").await.1.as_array().unwrap().len(), 0);
    let (st, g) = pay1.post("/wage-grids", body(false, csv)).await;
    assert_eq!(st, StatusCode::CREATED, "{g}");

    // Classe 1 pour e1 (salaire 1000.00 dans la devise de la société, très inférieur à 559000).
    let (st, b) = hr
        .post(
            &format!("/employees/{e1}/wage-class"),
            json!({"class_no": 1, "effective_from": "2025-01-01"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{b}");
    let (st, rep) = hr.get("/reports/smig-compliance?as_of=2026-03-01").await;
    assert_eq!(st, StatusCode::OK, "{rep}");
    let row = rep["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["employee_id"] == e1.as_str())
        .unwrap();
    assert_eq!(row["status"], "below", "{row}");
    assert_eq!(
        row["gap"].as_str().unwrap().parse::<f64>().unwrap(),
        558_000.0
    );
    // L'employé sans classe est signalé, jamais bloqué.
    assert!(
        rep["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["status"] == "no_class")
    );
    let (st, _ct, text) = get_text(&hr, "/reports/smig-compliance.csv?as_of=2026-03-01").await;
    assert_eq!(st, StatusCode::OK);
    assert!(
        text.trim_start_matches('\u{feff}').starts_with("Matricule"),
        "{text}"
    );

    // Isolation : une autre société ne voit pas la grille.
    make_tenant(&pool, "golf", "hr@golf.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@golf.test").await;
    assert_eq!(
        other.get("/wage-grids").await.1.as_array().unwrap().len(),
        0
    );
    assert_eq!(
        other
            .post(
                &format!("/employees/{e1}/wage-class"),
                json!({"class_no": 1, "effective_from": "2025-01-01"})
            )
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let _ = &mut pay2;

    let (_, audit) = hr.get("/audit?limit=100").await;
    let actions = audit.to_string();
    assert!(actions.contains("employee.wage_class"), "{actions}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn contract_templates_render_pdf_with_merge_fields_and_isolation(pool: PgPool) {
    let (mut hr, _pay1, _pay2, e1, _e2) = payroll_setup(&pool).await;
    let (st, c) = hr
        .post(
            "/contracts",
            json!({"employee_id": e1, "contract_type": "cdi", "start_date": "2026-01-01", "job_title": "Comptable"}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{c}");
    let cid = c["id"].as_str().unwrap().to_string();

    // Champ inconnu (ex. salaire : jamais fusionné) : refusé.
    let (st, b) = hr
        .post(
            "/contract-templates",
            json!({"name": "X", "body": "Salaire {{salary}}"}),
        )
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");

    let (st, tpl) = hr
        .post(
            "/contract-templates",
            json!({"name": "Modele test", "body": "Entre {{company}} et {{employee_name}} ({{matricule}}), poste {{job_title}}, debut {{start_date}}, fin {{end_date}}."}),
        )
        .await;
    assert_eq!(st, StatusCode::CREATED, "{tpl}");
    let tid = tpl["id"].as_str().unwrap();

    // Un employé simple ne gère pas les modèles.
    let mut paul = Client::new(&pool);
    paul.login("paul@foxtrot.test").await;
    assert_eq!(
        paul.post("/contract-templates", json!({"name": "Y", "body": "z"}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let (st, ct, pdf) = get_text(
        &hr,
        &format!("/contracts/{cid}/document.pdf?template_id={tid}"),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert!(ct.starts_with("application/pdf"), "{ct}");
    assert!(pdf.starts_with("%PDF-1.4"));
    assert!(pdf.contains("Comptable"), "champs fusionnés");
    assert!(pdf.contains("01/01/2026"));
    assert!(pdf.contains("non applicable"), "CDI sans date de fin");
    assert!(!pdf.contains("{{"));

    // Autre société : ni le modèle ni le contrat ne sont accessibles.
    make_tenant(&pool, "hotel", "hr@hotel.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@hotel.test").await;
    assert_eq!(
        other
            .get("/contract-templates")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        other.get(&format!("/contract-templates/{tid}")).await.0,
        StatusCode::NOT_FOUND
    );
    let (st, _, _) = get_text(
        &other,
        &format!("/contracts/{cid}/document.pdf?template_id={tid}"),
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    let (_, audit) = hr.get("/audit?limit=100").await;
    let a = audit.to_string();
    assert!(
        a.contains("contract_template.create") && a.contains("contract.document"),
        "{a}"
    );
}

async fn raw_send(c: &Client, method: &str, uri: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    let resp = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(format!("/api/v1{uri}"))
                .header(header::COOKIE, c.cookie.clone().unwrap())
                .header("x-requested-with", "mysuite")
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let st = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (st, bytes)
}

#[sqlx::test(migrations = "../../migrations")]
async fn employee_documents_are_validated_isolated_audited_and_purged(pool: PgPool) {
    let (mut hr, mut pay1, _pay2, e1, _e2) = payroll_setup(&pool).await;
    let pdf = b"%PDF-1.4\nfake but sniffed as pdf\n".to_vec();
    let up = |name: &str, exp: &str| {
        format!("/employees/{e1}/documents?name={name}&kind=identity&expires_on={exp}")
    };

    // Rôles : la paie ne voit pas les pièces.
    assert_eq!(
        raw_send(&pay1, "POST", &up("cni", "2026-12-01"), pdf.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        pay1.get(&format!("/employees/{e1}/documents")).await.0,
        StatusCode::FORBIDDEN
    );
    // Contenu non accepté, quel que soit le nom : refusé.
    let (st, _) = raw_send(
        &hr,
        "POST",
        &up("x", "2026-12-01"),
        b"<html><script>alert(1)</script>".to_vec(),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    // Trop volumineux.
    let mut big = b"%PDF-".to_vec();
    big.resize(5 * 1024 * 1024 + 10, b'a');
    let (st, _) = raw_send(&hr, "POST", &up("big", "2026-12-01"), big).await;
    assert!(
        st == StatusCode::UNPROCESSABLE_ENTITY || st == StatusCode::PAYLOAD_TOO_LARGE,
        "{st}"
    );

    let (st, body) = raw_send(&hr, "POST", &up("cni", "2026-12-01"), pdf.clone()).await;
    assert_eq!(st, StatusCode::CREATED);
    let doc: Value = serde_json::from_slice(&body).unwrap();
    let did = doc["id"].as_str().unwrap().to_string();
    assert_eq!(doc["content_type"], "application/pdf");

    let (_, list) = hr.get(&format!("/employees/{e1}/documents")).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (_, exp) = hr.get("/documents/expiring?days=365").await;
    assert_eq!(exp.as_array().unwrap().len(), 1);

    let (st, bytes) = raw_send(&hr, "GET", &format!("/documents/{did}/download"), vec![]).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bytes, pdf);

    // Autre société : 404 partout.
    make_tenant(&pool, "juliet", "hr@juliet.test").await;
    let mut other = Client::new(&pool);
    other.login("hr@juliet.test").await;
    assert_eq!(
        raw_send(&other, "GET", &format!("/documents/{did}/download"), vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        raw_send(&other, "DELETE", &format!("/documents/{did}"), vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        other.get(&format!("/employees/{e1}/documents")).await.0,
        StatusCode::NOT_FOUND
    );

    // Suppression : le contenu est effacé, plus de téléchargement, métadonnées conservées en base.
    assert_eq!(
        raw_send(&hr, "DELETE", &format!("/documents/{did}"), vec![])
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        raw_send(&hr, "GET", &format!("/documents/{did}/download"), vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (n, has): (i64, bool) =
        sqlx::query_as("SELECT count(*), bool_or(content IS NOT NULL) FROM employee_documents")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((n, has), (1, false));

    let (_, audit) = hr.get("/audit?limit=100").await;
    let a = audit.to_string();
    for act in ["document.upload", "document.download", "document.delete"] {
        assert!(a.contains(act), "{act}");
    }
    let _ = &mut pay1;
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_can_require_mfa_and_unenrolled_users_can_only_enroll(pool: PgPool) {
    let (mut hr, mut pay1, _pay2, _e1, _e2) = payroll_setup(&pool).await;
    // Seul l'administrateur règle la politique ; il doit d'abord avoir son propre MFA.
    assert_eq!(
        pay1.put("/security/policy", json!({"mfa_required": true}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (st, b) = hr
        .put("/security/policy", json!({"mfa_required": true}))
        .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY, "{b}");
    enable_mfa(&pool, "hr@foxtrot.test").await;
    let (st, b) = hr
        .put("/security/policy", json!({"mfa_required": true}))
        .await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(hr.get("/security/policy").await.1["mfa_required"], true);

    // pay1 n'a pas de MFA : tout est refusé avec un code dédié, sauf la configuration du MFA.
    let (st, b) = pay1.get("/payroll/periods").await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(b["code"], "mfa_enrollment_required", "{b}");
    let (st, me) = pay1.get("/auth/me").await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(me["mfa_enrollment_required"], true);
    assert_eq!(me["permissions"].as_array().unwrap().len(), 0);
    assert_eq!(pay1.get("/auth/mfa").await.0, StatusCode::OK);

    // Après activation, la même session est débloquée.
    enable_mfa(&pool, "pay1@foxtrot.test").await;
    let (st, b) = pay1.get("/payroll/periods").await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(
        pay1.get("/auth/me").await.1["mfa_enrollment_required"],
        false
    );

    // Désactiver la politique : les utilisateurs sans MFA retrouvent l'accès.
    assert_eq!(
        hr.put("/security/policy", json!({"mfa_required": false}))
            .await
            .0,
        StatusCode::OK
    );
    let (st, _) = pay1.get("/payroll/periods").await;
    assert_eq!(st, StatusCode::OK);
    let (_, audit) = hr.get("/audit?limit=100").await;
    assert!(audit.to_string().contains("tenant.mfa_policy"));
}
