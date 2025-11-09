fn main() {
    // Check that frontend assets have been built (required for embedding)
    // Only check in release mode since dev mode serves files directly
    #[cfg(not(debug_assertions))]
    {
        use std::path::Path;
        let frontend_dist = Path::new("frontend/dist");
        if !frontend_dist.exists() {
            eprintln!("\n❌ ERROR: Frontend assets not found at 'frontend/dist/'\n");
            eprintln!("The frontend must be built before compiling the Rust binary.\n");
            eprintln!("Quick fix:");
            eprintln!("  ./scripts/build.sh\n");
            eprintln!("Or manually:");
            eprintln!("  cd frontend");
            eprintln!("  npm install");
            eprintln!("  npm run build");
            eprintln!("  cd ..");
            eprintln!("  cargo build --release\n");
            std::process::exit(1);
        }
    }

    // Enables static linking of the vcruntime library on Windows builds
    static_vcruntime::metabuild();
}
