mod common;

use axum::{body::Body, http::Request};
use common::{send, test_app};
use painel::users;
use serde_json::{Value, json};

fn get(uri: &str, bearer: Option<&str>) -> Request<Body> {
    let mut b = Request::builder().uri(uri);
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    b.body(Body::empty()).unwrap()
}

async fn token_for(app: &common::TestApp, username: &str) -> String {
    let (_, _, body) = send(
        app,
        common::json_req(
            "POST",
            "/api/login",
            json!({"username": username, "password": "senha12345"}),
            None,
        ),
    )
    .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    v["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn users_paginated() {
    let app = test_app().await;
    for (u, n) in [("ana", "Ana"), ("bia", "Bia"), ("caio", "Caio"), ("duda", "Duda")] {
        users::create(&app.pool, u, n, "senha12345", false).await.unwrap();
    }
    let token = token_for(&app, "ana").await;
    let duda = users::find_by_username(&app.pool, "duda").await.unwrap().unwrap().0;
    users::set_active(&app.pool, duda.id, false).await.unwrap();

    let (status, _, body) = send(&app, get("/api/users?current=1&pageSize=20", Some(&token))).await;
    assert_eq!(status, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 3);
    assert_eq!(v["data"].as_array().unwrap().len(), 3);
    assert_eq!(v["data"][0]["name"], "ana");

    let (_, _, body) = send(&app, get("/api/users?current=2&pageSize=2", Some(&token))).await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["total"], 3);
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"][0]["name"], "caio");

    let (status, _, _) = send(&app, get("/api/users", None)).await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn peers_and_groups_empty() {
    let app = test_app().await;
    users::create(&app.pool, "ana", "Ana", "senha12345", false).await.unwrap();
    let token = token_for(&app, "ana").await;
    for uri in [
        "/api/peers?current=1&pageSize=20",
        "/api/device-group/accessible?current=1&pageSize=20",
    ] {
        let (status, _, body) = send(&app, get(uri, Some(&token))).await;
        assert_eq!(status, 200, "{uri}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v, json!({"total": 0, "data": []}));
        let (status, _, _) = send(&app, get(uri, None)).await;
        assert_eq!(status, 401, "{uri}");
    }
}
