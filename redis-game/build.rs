use bebop_tools as bebop;

fn main() {
    // download the bebop binary automatically and cache it into your target directory
    // it will automatically download the same version as the package you installed
    bebop::download_bebopc(std::path::PathBuf::from("target").join("bebopc"));
    // build all `.bop` schemas in `schemas` dir and make a new module `generated` in `src` with all of them.
    bebop::build_schema_dir("../schemas", "src/messages", &bebop::BuildConfig::default());
}
