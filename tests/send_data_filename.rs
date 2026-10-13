//! `send_data(..., filename:)` answers with Rails' Content-Disposition.
//!
//! ```ruby
//! send_data tarball, type: "application/gzip", filename: "backup.tar.gz"
//! ```
//!
//! Rails answers `attachment; filename="backup.tar.gz";
//! filename*=UTF-8''backup.tar.gz`; the runtime's `send_data` took only
//! `type:` and `disposition:`, so the action raised
//! `ArgumentError (unknown keyword: :filename)`. The expected headers are
//! `ActionDispatch::Http::ContentDisposition.format`'s output in
//! actionpack 8.1.4 for the same arguments.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;
#[path = "support/native_http.rs"]
mod native_http;

use roundhouse::project::BuildTarget;

const CONTROLLER: &str = r#"class DownloadsController < ApplicationController
  def tarball
    send_data "tar bytes", type: "application/gzip", filename: "backup.tar.gz"
  end

  def report
    send_data "report", type: "text/plain", disposition: "inline", filename: "my file (1).txt"
  end

  def utf8
    send_data "pdf", type: "application/pdf", filename: "zażółć.pdf"
  end

  def bare
    send_data "bare", type: "text/plain", disposition: "inline"
  end
end
"#;

const ASSERTIONS: &str = r#"
def get(path)
  Main.run_rack(
    "REQUEST_METHOD" => "GET",
    "PATH_INFO" => path,
    "QUERY_STRING" => "",
    "HTTP_HOST" => "localhost",
    "rack.input" => StringIO.new("")
  )
end

status, headers, body = get("/downloads/tarball")
raise "tarball status #{status}" unless status == 200
raise "tarball header #{headers.inspect}" unless headers["content-disposition"] == %q(attachment; filename="backup.tar.gz"; filename*=UTF-8''backup.tar.gz)
raise "tarball type #{headers.inspect}" unless headers["content-type"].to_s.start_with?("application/gzip")
raise "tarball body #{body.inspect}" unless body == ["tar bytes"]

status, headers, _body = get("/downloads/report")
raise "report status #{status}" unless status == 200
raise "report header #{headers.inspect}" unless headers["content-disposition"] == %q(inline; filename="my file %281%29.txt"; filename*=UTF-8''my%20file%20%281%29.txt)

status, headers, _body = get("/downloads/bare")
raise "bare status #{status}" unless status == 200
raise "bare header #{headers.inspect}" unless headers["content-disposition"] == "inline"

escaped = ActionController::Base.content_disposition("inline", %q(a"b;c&d#e$f+g.txt))
raise "escaping #{escaped}" unless escaped == %q(inline; filename="a%22b%3Bc%26d#e$f+g.txt"; filename*=UTF-8''a%22b%3Bc&d#e$f+g.txt)
status, headers, _body = get("/downloads/utf8")
raise "utf8 status #{status}" unless status == 200
raise "utf8 header #{headers.inspect}" unless headers["content-disposition"] == %q(attachment; filename="za%3F%3F%3F%3F.pdf"; filename*=UTF-8''za%C5%BC%C3%B3%C5%82%C4%87.pdf)
puts "send_data filename passed"
"#;

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  get \"/downloads/tarball\", to: \"downloads#tarball\"\n  get \"/downloads/report\", to: \"downloads#report\"\n  get \"/downloads/utf8\", to: \"downloads#utf8\"\n  get \"/downloads/bare\", to: \"downloads#bare\"\n",
        )
        .write("app/controllers/downloads_controller.rb", CONTROLLER)
}

#[test]
fn send_data_filename_writes_rails_content_disposition() {
    let run = overlay().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("send_data filename passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the native Spinel compiler; set SPINEL"]
fn send_data_filename_is_observable_in_the_native_http_response() {
    let (tree, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "analysis/emission errors:\n{}", errors.join("\n"));
    native_http::build(&tree);
    let server = native_http::Server::start(&tree);

    let expected = [
        ("/downloads/tarball", "attachment; filename=\"backup.tar.gz\"; filename*=UTF-8''backup.tar.gz"),
        ("/downloads/report", "inline; filename=\"my file %281%29.txt\"; filename*=UTF-8''my%20file%20%281%29.txt"),
        ("/downloads/utf8", "attachment; filename=\"za%3F%3F%3F%3F.pdf\"; filename*=UTF-8''za%C5%BC%C3%B3%C5%82%C4%87.pdf"),
        ("/downloads/bare", "inline"),
    ];
    for (route, header) in expected {
        let response = server.get(route);
        assert_eq!(response.status, 200, "{route}: {}", server.log());
        assert_eq!(
            response.headers.get("content-disposition").map(String::as_str),
            Some(header),
            "{route}: {:?}",
            response.headers
        );
    }
}
