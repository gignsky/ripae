// `sqlx::migrate!` embeds migrations/ at compile time; without this, adding
// a new migration file doesn't trigger a rebuild.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
