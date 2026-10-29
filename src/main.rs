//! ular-git binary entry point. All logic lives in the `ular_git` library crate
//! so it can be tested without launching the binary.

fn main() -> anyhow::Result<()> {
    ular_git::run()
}
