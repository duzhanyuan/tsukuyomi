extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_cors;

use {
    http::{
        header::{
            HeaderName,
            ACCESS_CONTROL_ALLOW_CREDENTIALS, //
            ACCESS_CONTROL_ALLOW_HEADERS,
            ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN,
            ACCESS_CONTROL_MAX_AGE,
            ACCESS_CONTROL_REQUEST_HEADERS,
            ACCESS_CONTROL_REQUEST_METHOD,
            COOKIE,
            HOST,
            ORIGIN,
        },
        Method, Request, Uri,
    },
    tsukuyomi::test::ResponseExt,
    tsukuyomi_cors::CORS,
};

#[test]
fn simple_request_with_default_configuration() -> tsukuyomi::test::Result<()> {
    let cors = CORS::new();

    let mut server = tsukuyomi::app!()
        .route(tsukuyomi::route!("/").reply(|| "hello"))
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    // without origin header
    let response = server.perform(
        Request::get("/") //
            .header(HOST, "localhost"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}

#[test]
fn simple_request_with_allow_origin() -> tsukuyomi::test::Result<()> {
    let cors = CORS::builder()
        .allow_origin(Uri::from_static("http://example.com"))
        .build();

    let mut server = tsukuyomi::app!()
        .route(tsukuyomi::route!("/").reply(|| "hello"))
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(
        response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?,
        "http://example.com"
    );

    // disallowed origin
    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn simple_request_with_allow_method() -> tsukuyomi::test::Result<()> {
    let cors = CORS::builder() //
        .allow_method(Method::GET)
        .build();

    let mut server = tsukuyomi::app!()
        .route(tsukuyomi::route!("/", methods = [GET, DELETE]).reply(|| "hello"))
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    // disallowed method
    let response = server.perform(
        Request::delete("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn simple_request_with_allow_credentials() -> tsukuyomi::test::Result<()> {
    let cors = CORS::builder() //
        .allow_credentials(true)
        .build();

    let mut server = tsukuyomi::app!()
        .route(tsukuyomi::route!("/").reply(|| "hello"))
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(COOKIE, "session=xxxx"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.body().to_utf8()?, "hello");
    assert_eq!(
        response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?,
        "http://example.com"
    );
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_CREDENTIALS)?, "true",);

    Ok(())
}

macro_rules! assert_methods {
    ($h:expr, [$($METHOD:ident),*]) => {{
        let h_str = $h.to_str()?;
        let methods: std::collections::HashSet<http::Method> = h_str
            .split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<_, _>>()?;
        let expected = vec![$(http::Method::$METHOD),*].into_iter().collect();
        assert_eq!(methods, expected);
    }};
}

macro_rules! assert_headers {
    ($h:expr, [$($name:expr),*]) => {{
        let h_str = $h.to_str()?;
        let headers: std::collections::HashSet<http::header::HeaderName> = h_str
            .split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<_, _>>()?;
        let expected = vec![$($name),*].into_iter().collect();
        assert_eq!(headers, expected);
    }};
}

#[test]
fn preflight_with_default_configuration() -> tsukuyomi::test::Result<()> {
    let cors = CORS::new();

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::route!("/") //
                .reply(|| "hello"),
        ) //
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");
    assert_methods!(
        response.header(ACCESS_CONTROL_ALLOW_METHODS)?,
        [GET, POST, OPTIONS]
    );

    Ok(())
}

#[test]
fn preflight_with_allow_origin() -> tsukuyomi::test::Result<()> {
    let cors = CORS::builder()
        .allow_origin(Uri::from_static("http://example.com"))
        .build();

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::route!("/") //
                .reply(|| "hello"),
        ) //
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_with_allow_method() -> tsukuyomi::test::Result<()> {
    let cors = CORS::builder().allow_method(Method::GET).build();

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::route!("/") //
                .reply(|| "hello"),
        ) //
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "DELETE"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_with_allow_headers() -> tsukuyomi::test::Result<()> {
    let x_api_key = HeaderName::from_static("x-api-key");

    let cors = CORS::builder() //
        .allow_header(x_api_key.clone())
        .build();

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::route!("/") //
                .reply(|| "hello"),
        ) //
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(ACCESS_CONTROL_REQUEST_HEADERS, "x-api-key"),
    )?;
    assert_eq!(response.status(), 204);
    assert_headers!(response.header(ACCESS_CONTROL_ALLOW_HEADERS)?, [x_api_key]);

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.org")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(ACCESS_CONTROL_REQUEST_HEADERS, "authorization"),
    )?;
    assert_eq!(response.status(), 403);

    Ok(())
}

#[test]
fn preflight_max_age() -> tsukuyomi::test::Result<()> {
    const SECS_PER_DAY: i64 = 60 * 60 * 24;

    let cors = CORS::builder() //
        .max_age(std::time::Duration::from_secs(SECS_PER_DAY as u64))
        .build();

    let mut server = tsukuyomi::app!()
        .route(tsukuyomi::route!("/").reply(|| "hello"))
        .with(cors)
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::options("*")
            .header(HOST, "localhost")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(
        response.header(ACCESS_CONTROL_MAX_AGE)?,
        SECS_PER_DAY.to_string().as_str()
    );

    Ok(())
}

#[test]
fn as_extractor() -> tsukuyomi::test::Result<()> {
    let cors = CORS::new();

    let mut server = tsukuyomi::app!()
        .route(
            tsukuyomi::route!("/cors", methods = [GET, OPTIONS])
                .with(cors.clone())
                .reply(|| "cors"),
        ) //
        .route(
            tsukuyomi::route!("/nocors") //
                .reply(|| "nocors"),
        ) //
        .route(
            tsukuyomi::route!("*", method = OPTIONS)
                .with(cors)
                .reply(|| ()),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/cors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("/cors") //
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("*")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::get("/nocors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}

#[test]
fn as_scope_modifier() -> tsukuyomi::test::Result<()> {
    let cors = CORS::new();

    let mut server = tsukuyomi::app!()
        .mount(
            tsukuyomi::scope!("/cors")
                .with(cors.clone())
                .route(tsukuyomi::route!("/").reply(|| "cors")),
        ) //
        .route(
            tsukuyomi::route!("/nocors") //
                .reply(|| "nocors"),
        ) //
        .route(
            tsukuyomi::route!("*", method = OPTIONS)
                .with(cors)
                .reply(|| ()),
        ) //
        .build_server()?
        .into_test_server()?;

    let response = server.perform(
        Request::get("/cors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert_eq!(response.status(), 200);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("/cors") //
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::options("*")
            .header(ORIGIN, "http://example.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET"),
    )?;
    assert_eq!(response.status(), 204);
    assert_eq!(response.header(ACCESS_CONTROL_ALLOW_ORIGIN)?, "*");

    let response = server.perform(
        Request::get("/nocors") //
            .header(ORIGIN, "http://example.com"),
    )?;
    assert!(!response.headers().contains_key(ACCESS_CONTROL_ALLOW_ORIGIN));

    Ok(())
}