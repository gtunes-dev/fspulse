// Enables static linking of the vcruntime library on Windows builds
fn main() {
    static_vcruntime::metabuild();
}