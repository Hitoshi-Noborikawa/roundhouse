//! A `constraints` block keyed by a request attribute keeps its routes.
//!
//! An app's services routes sit inside
//! `constraints host: Rails.configuration.app_host_constraint,
//! protocol: "https://" do … end`. `host:` and `protocol:` name no path
//! segment: Rails asks the request (`request.host`, `request.protocol`).
//! The block reader took every key for a path-segment regex and refused a
//! value that is no literal, so the survey dropped the whole block and its
//! routes answered 404.
//!
//! The request is not checked, as a guard object's is not
//! (`constraints(SomeGuard.new)`); a segment constraint still refuses a
//! value that is no literal.

use roundhouse::App;
use roundhouse::ingest::ingest_routes;
use roundhouse::lower::routes::{FlatRoute, flatten_routes};

fn routes(source: &str) -> Result<Vec<FlatRoute>, String> {
    let mut app = App::default();
    app.routes = ingest_routes(
        format!("Rails.application.routes.draw do\n{source}\nend\n").as_bytes(),
        "config/routes.rb",
    )
    .map_err(|e| e.to_string())?;
    Ok(flatten_routes(&app))
}

#[test]
fn a_host_and_protocol_block_keeps_its_routes() {
    let r = routes(
        r#"constraints host: Rails.configuration.app_host_constraint, protocol: "https://" do
  scope :services, module: :services do
    scope :connected_paypal do
      get "partner_referral/:shop_id", controller: "partner_referral", action: :create, as: :partner_referral
    end
  end
end
get "/ping", to: "pings#show""#,
    )
    .expect("ingest routes");
    let paths: Vec<&str> = r.iter().map(|route| route.path.as_str()).collect();
    assert_eq!(paths, ["/services/connected_paypal/partner_referral/:shop_id", "/ping"], "{r:#?}");
    assert!(r.iter().all(|route| route.refused.is_none()), "{r:#?}");
    assert!(r[0].seg_patterns.is_empty(), "{r:#?}");
}

#[test]
fn a_segment_constraint_that_is_no_literal_stays_a_gap() {
    let err = routes(
        r#"constraints id: Rails.configuration.id_pattern do
  get "/things/:id", to: "things#show"
end"#,
    )
    .expect_err("a segment constraint");
    assert!(err.contains("is not a literal regex or a plain name"), "{err}");
}
