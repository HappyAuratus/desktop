use ora_domain::ProjectId;

/// Builds a shared domain identifier so the web server stays wired to the common model crate.
fn bootstrap_project_id() -> ProjectId {
    ProjectId::new("web-bootstrap")
}

fn main() {
    let _ = bootstrap_project_id();
}
