//! Authentication server functions with database integration.

use leptos::prelude::*;
use leptos::server;
use serde::{Deserialize, Serialize};

/// Public session data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    pub github_id: i64,
    pub github_login: String,
    pub github_avatar_url: Option<String>,
    pub plan_tier: String,
}

#[server]
pub async fn get_session() -> Result<Option<SessionUser>, server_fn::ServerFnError> {
    use axum_extra::extract::cookie::CookieJar;
    use leptos_axum::extract;
    use std::sync::Arc;

    let jar: CookieJar = extract().await?;
    let Some(token) = jar.get("monochange_session").map(|c| c.value().to_string()) else {
        return Ok(None);
    };

    let state: Arc<monochange_app_api::AppState> = expect_context();
    let claims = monochange_app_api::verify_token(&state.jwt_secret, &token)
        .map_err(|_| server_fn::ServerFnError::new("Invalid session"))?;

    let client = monochange_app_db::get_client(&state.db)
        .await
        .map_err(|e| server_fn::ServerFnError::new(format!("DB: {e}")))?;

    let users = monochange_app_db::models::User::where_col(|u| u.id.equal(claims.sub))
        .run(&client)
        .await
        .map_err(|e| server_fn::ServerFnError::new(format!("Query: {e}")))?;

    Ok(users.first().map(|u| SessionUser {
        github_id: u.github_id,
        github_login: u.github_login.clone(),
        github_avatar_url: u.github_avatar_url.clone(),
        plan_tier: u.plan_tier.clone(),
    }))
}

#[server]
pub async fn get_login_url() -> Result<String, server_fn::ServerFnError> {
    use std::sync::Arc;

    let state: Arc<monochange_app_api::AppState> = expect_context();

    Ok(format!(
        "https://github.com/login/oauth/authorize?client_id={}&state={}&scope=user:email,read:org",
        state.github_client_id,
        uuid::Uuid::new_v4(),
    ))
}

#[server]
pub async fn exchange_code(
    code: String,
    state_param: String,
) -> Result<SessionUser, server_fn::ServerFnError> {
    use axum_extra::extract::cookie::Cookie;
    use leptos_axum::ResponseOptions;
    use monochange_app_db::models::User;
    use std::sync::Arc;

    let state: Arc<monochange_app_api::AppState> = expect_context();
    let _ = state_param;

    // Exchange code for access token
    let http = reqwest::Client::new();
    let token_response: serde_json::Value = http
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": state.github_client_id,
            "client_secret": state.github_client_secret,
            "code": code,
        }))
        .send().await.map_err(|e| server_fn::ServerFnError::new(format!("Token: {e}")))?
        .json().await.map_err(|e| server_fn::ServerFnError::new(format!("Parse: {e}")))?;

    let access_token = token_response["access_token"].as_str()
        .ok_or_else(|| server_fn::ServerFnError::new("No access_token"))?
        .to_string();

    let gh_user: serde_json::Value = http
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", "monochange-app")
        .send().await.map_err(|e| server_fn::ServerFnError::new(format!("User: {e}")))?
        .json().await.map_err(|e| server_fn::ServerFnError::new(format!("Parse: {e}")))?;

    let github_id = gh_user["id"].as_i64()
        .ok_or_else(|| server_fn::ServerFnError::new("Invalid GitHub user"))?;
    let login = gh_user["login"].as_str().unwrap_or("unknown").to_string();
    let avatar = gh_user["avatar_url"].as_str().map(String::from);

    // Upsert user
    let db = monochange_app_db::get_client(&state.db)
        .await.map_err(|e| server_fn::ServerFnError::new(format!("DB: {e}")))?;

    let existing = User::where_col(|u| u.github_id.equal(github_id))
        .run(&db).await.map_err(|e| server_fn::ServerFnError::new(format!("Query: {e}")))?;

    let db_user = if let Some(mut eu) = existing.into_iter().next() {
        eu.github_access_token = access_token;
        eu.github_login = login.clone();
        eu.github_avatar_url = avatar.clone();
        eu.save(&db).await.map_err(|e| server_fn::ServerFnError::new(format!("Save: {e}")))?;
        eu
    } else {
        let mut nu = User::new();
        nu.github_id = github_id;
        nu.github_login = login.clone();
        nu.github_avatar_url = avatar.clone();
        nu.github_access_token = access_token;
        nu.plan_tier = "free".to_string();
        nu.save(&db).await.map_err(|e| server_fn::ServerFnError::new(format!("Create: {e}")))?;
        nu
    };

    // JWT
    let token = monochange_app_api::create_token(
        &state.jwt_secret, db_user.id, db_user.github_id, &db_user.github_login,
    ).map_err(|e| server_fn::ServerFnError::new(format!("JWT: {e}")))?;

    // Cookie
    let resp = expect_context::<ResponseOptions>();
    let cookie = Cookie::build(("monochange_session", token))
        .path("/").http_only(true).secure(false)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    resp.append_header(
        axum::http::HeaderName::from_static("set-cookie"),
        axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
            .unwrap_or(axum::http::HeaderValue::from_static("")),
    );

    Ok(SessionUser {
        github_id: db_user.github_id,
        github_login: db_user.github_login.clone(),
        github_avatar_url: db_user.github_avatar_url.clone(),
        plan_tier: db_user.plan_tier.clone(),
    })
}

#[server]
pub async fn logout() -> Result<(), server_fn::ServerFnError> {
    use axum_extra::extract::cookie::Cookie;
    use leptos_axum::ResponseOptions;

    let resp = expect_context::<ResponseOptions>();
    let cookie = Cookie::build(("monochange_session", ""))
        .path("/").http_only(true)
        .max_age(time::Duration::seconds(0))
        .build();

    resp.append_header(
        axum::http::HeaderName::from_static("set-cookie"),
        axum::http::HeaderValue::from_str(&cookie.encoded().to_string())
            .unwrap_or(axum::http::HeaderValue::from_static("")),
    );

    Ok(())
}
