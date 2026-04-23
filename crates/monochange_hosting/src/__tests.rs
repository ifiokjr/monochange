#![forbid(clippy::indexing_slicing)]

use httpmock::Method;
use httpmock::MockServer;

use super::*;

#[test]
fn get_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::GET).path("/test");
		then.status(500).body("Internal Server Error");
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<String> =
		get_json(&client, &headers, &server.url("/test"), "test");

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API GET"));
	assert!(error.contains("500"));
	mock.assert();
}

#[test]
fn get_optional_json_returns_none_for_404() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::GET).path("/missing");
		then.status(404);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<Option<String>> =
		get_optional_json(&client, &headers, &server.url("/missing"), "test");

	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
	mock.assert();
}

#[test]
fn get_optional_json_returns_error_for_non_404_non_success() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::GET).path("/bad");
		then.status(500);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<Option<String>> =
		get_optional_json(&client, &headers, &server.url("/bad"), "test");

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API GET"));
	assert!(error.contains("500"));
	mock.assert();
}

#[test]
fn post_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::POST).path("/test");
		then.status(422).body("Validation Failed");
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> =
		post_json(&client, &headers, &server.url("/test"), &body, "test");

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API POST"));
	assert!(error.contains("422"));
	mock.assert();
}

#[test]
fn put_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::PUT).path("/test");
		then.status(403);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> =
		put_json(&client, &headers, &server.url("/test"), &body, "test");

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API PUT"));
	assert!(error.contains("403"));
	mock.assert();
}

#[test]
fn patch_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(Method::PATCH).path("/test");
		then.status(409);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> =
		patch_json(&client, &headers, &server.url("/test"), &body, "test");

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API PATCH"));
	assert!(error.contains("409"));
	mock.assert();
}
