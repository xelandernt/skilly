use super::*;

pub(super) fn run_util_venv(path: &Path, detailed: bool) -> Result<()> {
    let skills = crate::core::discover_venv_skills(path)?;
    println!("Found {} skills:", skills.len());
    for skill in skills {
        println!(
            "{}[{}]:\n{}",
            skill.name,
            skill
                .package_reference()
                .unwrap_or_else(|| "unknown".to_string()),
            skill.description
        );
        if detailed {
            println!("\tResources:");
            for resource in skill.resources {
                let content_length = String::from_utf8_lossy(&resource.raw).lines().count();
                println!(
                    "\t\t{} [{}]: {} lines.",
                    resource.relative_path, resource.kind, content_length
                );
            }
        }
    }
    Ok(())
}
