use super::*;

pub(super) fn run_create(directory: &Path, mut options: CreateOptions) -> Result<()> {
    let interactive = is_interactive_terminal();
    if interactive {
        let Some(submission) = run_create_tui(directory, options)? else {
            println!("Cancelled without creating skill");
            return Ok(());
        };
        options = submission;
    }

    let name = options
        .name
        .context("Skill name is required outside an interactive terminal")?;
    let description = options
        .description
        .context("Skill description is required outside an interactive terminal")?;
    let skill = SkillData {
        name: name.clone(),
        description,
        path: None,
        body: options
            .instructions
            .unwrap_or_else(|| DEFAULT_CREATE_INSTRUCTIONS.to_string()),
        raw: Vec::new(),
        license: options.license,
        compatibility: options.compatibility,
        metadata: parse_metadata(&options.metadata)?,
        allowed_tools: options.allowed_tools,
        resources: Vec::new(),
        resource_warnings: Vec::new(),
        source: SKILLY_UNKNOWN_SOURCE.to_string(),
        package_name: None,
        package_version: None,
        repository_provider: None,
        repository_url: None,
        repository_commit_sha: None,
        package_ecosystem: None,
    };
    skill.validate()?;

    let installed = if options.overwrite {
        skill.replace_to(directory, None)?
    } else {
        skill.install_to(directory, None, false)?
    };
    let root = installed
        .path
        .as_deref()
        .map(Path::new)
        .context("Created skill has no directory")?;
    for (requested, child) in [
        (options.with_scripts, "scripts"),
        (options.with_references, "references"),
        (options.with_assets, "assets"),
    ] {
        if requested {
            fs::create_dir_all(root.join(child))?;
        }
    }
    println!("Created {} at {}", installed.name, root.display());
    Ok(())
}

pub(super) fn build_project_environment(
    skills_directory: &Path,
    selection: &ScanDependencySelection,
) -> ProjectEnvironment {
    let sources = vec![
        ProjectSource::Python(PythonSourceSettings {
            pyproject_toml_path: PathBuf::from("pyproject.toml"),
            venv_path: PathBuf::from(".venv"),
            include_project_dependencies: selection.include_project_dependencies,
            dependency_groups: selection.dependency_groups.clone(),
            optional_dependencies: selection.optional_dependencies.clone(),
        }),
        ProjectSource::Node(NodeSourceSettings {
            package_json_path: PathBuf::from("package.json"),
            node_modules_path: PathBuf::from("node_modules"),
            include_dependencies: selection.include_node_dependencies,
            include_dev_dependencies: selection.include_node_dev_dependencies,
            include_optional_dependencies: selection.include_node_optional_dependencies,
        }),
        ProjectSource::Maven(MavenSourceSettings::default()),
    ];
    ProjectEnvironment {
        directory: skills_directory.to_path_buf(),
        sources,
    }
}
