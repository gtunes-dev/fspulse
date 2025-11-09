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

    // Capture build metadata for runtime version display
    capture_build_info();

    // Enables static linking of the vcruntime library on Windows builds
    static_vcruntime::metabuild();
}

fn capture_build_info() {
    // Capture build timestamp
    let build_timestamp = chrono::Utc::now().to_rfc3339();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);

    // Try to get git commit hash
    // Priority: 1) git command, 2) GIT_COMMIT env var, 3) GITHUB_SHA env var, 4) "unknown"
    let git_commit = get_git_commit()
        .or_else(|| std::env::var("GIT_COMMIT").ok())
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_COMMIT={}", git_commit);

    // Try to get git commit short hash (first 7 characters)
    let git_commit_short = if git_commit.len() >= 7 {
        git_commit[..7].to_string()
    } else {
        git_commit.clone()
    };
    println!("cargo:rustc-env=GIT_COMMIT_SHORT={}", git_commit_short);

    // Try to get git branch
    let git_branch = get_git_branch()
        .or_else(|| std::env::var("GIT_BRANCH").ok())
        .or_else(|| std::env::var("GITHUB_REF_NAME").ok())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_BRANCH={}", git_branch);

    // Re-run if .git/HEAD changes (detects branch switches or new commits)
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn get_git_commit() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

fn get_git_branch() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}
