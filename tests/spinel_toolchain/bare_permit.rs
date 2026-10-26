//! The Spinel half of `emit_and_run/bare_permit.rs`, through `spin build`
//! and the native server; its database starts empty, so the counts are 0.

use super::bare_permit_contract as contract;
use super::native_http;
use roundhouse::project::BuildTarget;

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn a_bare_permit_runs_natively() {
    let (tree, errors) = contract::overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "analysis/emission errors:\n{}", errors.join("\n"));
    native_http::build(&tree);
    let server = native_http::Server::start(&tree);

    let full = server.get(contract::QUERY);
    assert_eq!(full.status, 200, "{}", full.body);
    assert!(full.body.contains("<p id=\"q\">&lt;rails&gt;</p>"), "{}", full.body);
    assert!(full.body.contains("<p id=\"other\">one</p>"), "{}", full.body);
    assert!(full.body.contains("<p id=\"log\">Alice|1||2|0</p>"), "{}", full.body);

    let bare = server.get("/finds");
    assert_eq!(bare.status, 200, "{}", bare.body);
    assert!(bare.body.contains("<p id=\"log\">(All)|||1|0</p>"), "{}", bare.body);
}
