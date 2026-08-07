use serde_json::{json, Value};
use zw_db::filters::parse_filter;

async fn pool() -> (zw_db::DbPool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("zwdb_test_srv_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let pool = zw_db::DbPool::open(db_path.to_str().unwrap()).await.unwrap();
    pool.migrate().await.unwrap();
    (pool, dir)
}

fn payloads() -> Vec<Value> {
    vec![
        json!({"name": "Dell XPS 13", "brand": "Dell", "price": 1200, "body": "the quick brown fox"}),
        json!({"name": "MacBook Air", "brand": "Apple", "price": 1300, "body": "lightweight laptop machine"}),
        json!({"name": "ThinkPad", "brand": "Lenovo", "price": 900, "body": "durable workhorse"}),
    ]
}

#[tokio::test]
async fn set_and_patch_ops() {
    let (pool, dir) = pool().await;
    let board = pool.srv_app_create("owner", "t", None, true, None).await.unwrap();
    let seq = pool.srv_record_insert(&board, &payloads()[0], Some("alice"), false).await.unwrap();

    pool.srv_record_set(&board, seq, &json!({"name": "Dell XPS 15", "brand": "Dell", "price": 799}), Some("alice")).await.unwrap();
    let rec = pool.srv_record_get(&board, seq).await.unwrap().unwrap();
    assert_eq!(rec.payload["name"], "Dell XPS 15");
    assert_eq!(rec.writer.as_deref(), Some("alice"));

    let merged = pool.srv_record_patch(&board, seq, &json!({"$set": {"attrs.color": "black"}, "$inc": {"price": 50}, "$dec": {"price": 10}, "$mul": {"price": 2}, "$unset": ["brand"]})).await.unwrap();
    assert_eq!(merged["attrs"]["color"], "black");
    assert_eq!(merged["price"], json!(1678.0));
    assert!(merged.get("brand").is_none());

    let merged = pool.srv_record_patch(&board, seq, &json!({"notes": "hello"})).await.unwrap();
    assert_eq!(merged["notes"], "hello");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn unique_duplicates_upsert_writer() {
    let (pool, dir) = pool().await;
    let board = pool.srv_app_create("owner", "t", None, true, Some("$.sku")).await.unwrap();

    let s1 = pool.srv_record_insert(&board, &json!({"sku": "A1", "qty": 1}), Some("alice"), false).await.unwrap();
    let err = pool.srv_record_insert(&board, &json!({"sku": "A1", "qty": 2}), Some("bob"), false).await;
    assert!(err.is_err(), "duplicate should be rejected");

    let s2 = pool.srv_record_insert(&board, &json!({"sku": "B1", "qty": 5}), None, false).await.unwrap();
    assert_eq!(s2, s1 + 1);

    let up = pool.srv_record_insert(&board, &json!({"sku": "A1", "qty": 9}), Some("bob"), true).await.unwrap();
    assert_eq!(up, s1, "upsert keeps the same seq");
    let rec = pool.srv_record_get(&board, s1).await.unwrap().unwrap();
    assert_eq!(rec.payload["qty"], 9);

    // writer filter
    let f = parse_filter(&json!({"writer": {"eq": "bob"}})).unwrap();
    let rows = pool.srv_record_filter(&board, &f.conds, None, 50, 0).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].writer.as_deref(), Some("bob"));

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn offset_ngram_substring_and_snippet() {
    let (pool, dir) = pool().await;
    let board = pool.srv_app_create("owner", "t", None, true, None).await.unwrap();
    for p in payloads() {
        pool.srv_record_insert(&board, &p, None, false).await.unwrap();
    }
    // ngram substring
    let hits = pool.srv_record_search(&board, "lapt", &[], 50, 0, true).await.unwrap();
    assert_eq!(hits.len(), 1, "ngram substring 'lapt' should match MacBook Air, got {:?}", hits);
    assert!(hits[0].snippet.is_some());

    // offset paging in list
    let page1 = pool.srv_record_list(&board, 2, None, 0).await.unwrap();
    let page2 = pool.srv_record_list(&board, 2, None, 2).await.unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);
    assert_ne!(page1[0].seq, page2[0].seq);

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn patch_rejects_wrong_shape_and_missing_record() {
    let (pool, dir) = pool().await;
    let board = pool.srv_app_create("owner", "t", None, true, None).await.unwrap();
    let seq = pool.srv_record_insert(&board, &json!({"n": 1}), None, false).await.unwrap();
    let err = pool.srv_record_patch(&board, seq, &json!([1, 2])).await;
    assert!(err.is_err(), "array patch should be rejected");
    let err = pool.srv_record_patch(&board, 9999, &json!({"$set": {"a": 1}})).await;
    assert!(err.is_err());
    std::fs::remove_dir_all(&dir).ok();
}