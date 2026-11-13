fn main() {
    let document = tulpar_api::openapi::document();
    let json = serde_json::to_string(&document).expect("serialize OpenAPI");
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        format!(
            "{}/../../packages/shared-types/openapi.json",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    std::fs::write(&path, format!("{json}\n")).expect("write OpenAPI document");
    println!("wrote {path}");
}
