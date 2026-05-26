//! Live smoke test against an active Amplify sandbox.
//!
//! `#[ignore]` because it needs:
//!   * `amplify_outputs.json` embedded (or `MELETE_AMPLIFY_OUTPUTS` set)
//!   * `MELETE_TEST_USERNAME` + `MELETE_TEST_PASSWORD` env vars set
//!     to a Cognito user the sandbox knows about
//!
//! Run with:
//!   ```sh
//!   MELETE_TEST_USERNAME=alice@example.com \
//!   MELETE_TEST_PASSWORD=... \
//!   cargo test -p journal-storage --features remote \
//!       --test smoke_signin -- --ignored --nocapture
//!   ```

use melete_storage::remote_template_store::store::{RemoteTemplateOps, RemoteTemplateStore};

#[test]
#[ignore]
fn smoke_sign_in_then_list_public() {
    let username =
        std::env::var("MELETE_TEST_USERNAME").expect("set MELETE_TEST_USERNAME for smoke test");
    let password =
        std::env::var("MELETE_TEST_PASSWORD").expect("set MELETE_TEST_PASSWORD for smoke test");

    let mut s = RemoteTemplateStore::connect().expect("connect");
    s.sign_in(&username, &password).expect("sign_in");
    assert!(s.is_signed_in());

    let templates = s.list_public_page_templates().expect("list templates");
    eprintln!("public page templates: {}", templates.len());
    let notebooks = s.list_public_notebook_templates().expect("list notebooks");
    eprintln!("public notebook templates: {}", notebooks.len());
    let brushes = s.list_public_brushes().expect("list brushes");
    eprintln!("public brushes: {}", brushes.len());
}
