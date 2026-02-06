use anyhow::Result;
use vergen_gitcl::{Emitter, GitclBuilder};

fn main() -> Result<()> {
    let gitcl = GitclBuilder::default().describe(true, true, None).build()?;
    Emitter::new().fail_on_error().add_instructions(&gitcl)?.emit()
}
