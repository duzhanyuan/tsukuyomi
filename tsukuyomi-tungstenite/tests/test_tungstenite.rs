extern crate http;
extern crate tsukuyomi;
extern crate tsukuyomi_tungstenite;
extern crate version_sync;

use {
    http::{
        header::{
            CONNECTION, //
            HOST,
            SEC_WEBSOCKET_ACCEPT,
            SEC_WEBSOCKET_KEY,
            SEC_WEBSOCKET_VERSION,
            UPGRADE,
        },
        Request,
    },
    tsukuyomi::{
        config::prelude::*, //
        App,
    },
    tsukuyomi_server::test::ResponseExt,
    tsukuyomi_tungstenite::Ws,
};

#[test]
fn test_version_sync() {
    version_sync::assert_html_root_url_updated!("src/lib.rs");
}

#[test]
fn test_handshake() -> tsukuyomi_server::Result<()> {
    let app = App::create(
        path!("/ws") //
            .to(endpoint::get().reply(Ws::new(|_| Ok(())))),
    )?;
    let mut server = tsukuyomi_server::test::server(app)?;

    let response = server.perform(
        Request::get("/ws")
            .header(HOST, "localhost:4000")
            .header(CONNECTION, "upgrade")
            .header(UPGRADE, "websocket")
            .header(SEC_WEBSOCKET_VERSION, "13")
            .header(SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ=="),
    )?;
    assert_eq!(response.status(), 101);
    assert_eq!(response.header(CONNECTION)?, "upgrade");
    assert_eq!(response.header(UPGRADE)?, "websocket");
    assert_eq!(
        response.header(SEC_WEBSOCKET_ACCEPT)?,
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
    );

    Ok(())
}

// TODO: add check whether the task to handle upgraded connection is spawned
