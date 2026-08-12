use serde_json::json;
use zw_recipe::{Engine, EventContext, Secrets, parse_recipe};
use zw_recipe::actions::match_public;

fn recipe_json() -> serde_json::Value {
    json!({
        "name": "paid-invoice",
        "when": "record.created",
        "match": {"$.status": {"eq": "paid"}},
        "actions": [
            {"$compute": {"total": "$.price * $.qty"}},
            {"$compute": {"label": "concat(upper($.customer), '-', $.id)"}},
            {"$push": {"path": "$.tags", "value": "auto"}}
        ]
    })
}

#[test]
fn parse_and_kind() {
    let r = parse_recipe(&recipe_json()).unwrap();
    assert_eq!(r.name, "paid-invoice");
    assert_eq!(r.event_kind(), Some(zw_recipe::EventKind::Created));
}

#[test]
fn match_filter_combos() {
    let p = json!({"status": "paid", "qty": 5, "tags": ["a"], "sku": 10});
    assert!(match_public(&p, &json!({"$.status": {"eq": "paid"}})).unwrap());
    assert!(match_public(&p, &json!({"$.qty": {"gte": 3}, "$.tags": {"contains": "a"}})).unwrap());
    assert!(match_public(&p, &json!({"$.sku": {"in": [1, 10, 20]}})).unwrap());
    assert!(!match_public(&p, &json!({"$.qty": {"lt": 1}})).unwrap());
}

#[test]
fn secrets_do_not_leak_in_report() {
    let mut s = Secrets::new();
    s.insert("KEY", "sekret");
    assert!(!serde_json::to_string(&s.masked()).unwrap().contains("sekret"));
    assert_eq!(s.substitute("Bearer ${KEY}"), "Bearer sekret");
}

#[tokio::test]
async fn engine_dry_run_applies_compute() {
    let dir = std::env::temp_dir().join(format!("zwrecipe_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = zw_db::DbPool::open(dir.join("test.db").to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();
    db.migrate_session().await.unwrap();
    let board = db.srv_app_create("owner", "t", None, true, None).await.unwrap();

    let recipe = parse_recipe(&recipe_json()).unwrap();
    let ctx = EventContext::record_created(&board, 1, &json!({"status": "paid", "price": 10, "qty": 3, "customer": "acme", "id": 7}), None);
    let engine = Engine { db: &db };
    let outcomes = engine.run_recipes(&[recipe], &Secrets::new(), &ctx, true).await;
    assert_eq!(outcomes.len(), 1);
    let (name, out) = &outcomes[0];
    assert_eq!(name, "paid-invoice");
    assert!(out.matched);
    assert!(out.report.actions_run.iter().any(|a| a.contains("Compute")));

    std::fs::remove_dir_all(&dir).ok();
}