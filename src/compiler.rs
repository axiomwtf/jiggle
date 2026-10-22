use crate::build_scripts::generate_build_sh;
use crate::bundler::create_bundle;
use crate::cli::Commands;
use crate::deps::generate_dependency_files;
use crate::error::JglcError;
use crate::manifest::Manifest;
use crate::parser::parse_jgl;
use crate::vfs::build_vfs;
use std::fs;
use std::path::Path;

pub fn run(args: crate::cli::Cli) -> Result<(), JglcError> {
    match args.command {
        Commands::Compile {
            project,
            output,
            bundle_only,
            target_win,
        } => compile_project(&project, &output, bundle_only, target_win),
        Commands::Validate { project } => validate_project(&project),
        Commands::Generate {
            project,
            output,
            target_win,
        } => generate_build(&project, &output, target_win),
    }
}
fn compile_project(
    project_path: &Path,
    output_dir: &Path,
    bundle_only: bool,
    target_win: bool,
) -> Result<(), JglcError> {
    // Step 1: Read project.jgl
    let content = fs::read_to_string(project_path).map_err(JglcError::Io)?;

    // Step 2: Parse all blocks
    let jgl = parse_jgl(&content)?;

    // Step 3: Parse manifest
    let manifest = if let Some(manifest_block) = &jgl.manifest {
        Manifest::parse(manifest_block)?
    } else {
        return Err(JglcError::Compiler(
            "No project._jgl manifest block found".to_string(),
        ));
    };

    // Step 4: Resolve imports
    let base_dir = project_path.parent().unwrap_or(Path::new("."));
    let mut manifest = manifest;
    let imported_blocks = crate::manifest::resolve_imports(&mut manifest, base_dir)?;

    // Step 5: Build VFS from all blocks (including imported)
    let mut all_blocks = jgl.blocks;
    all_blocks.extend(imported_blocks);
    let mut vfs = build_vfs(all_blocks, &manifest)?;

    // Step 6: Generate dependency files and add to VFS
    generate_dependency_files(&manifest, &manifest.custom_languages, &mut vfs)?;

    // Step 7: Check for dep file name collisions with custom languages
    // This is already handled in generate_dependency_files

    // Step 8: Create output directory if needed
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).map_err(JglcError::Io)?;
    }

    // Step 9: Create the tar.xz bundle
    create_bundle(&vfs, output_dir)?;

    // Step 10: Generate build scripts
    generate_build_sh(
        &manifest,
        &vfs,
        &manifest.custom_languages,
        output_dir,
        target_win,
    )?;
    // NOTE: .bat generation removed - use --target-win flag for Windows cross-compilation

    // Step 11: Write the manifest bundle file
    if let Some(manifest_block) = &jgl.manifest {
        crate::bundler::write_manifest_bundle(&manifest_block.body, output_dir)?;
    }

    if bundle_only {
        println!(
            "jglc: Bundle created at {}",
            output_dir.join("project.tar.xz").display()
        );
        println!(
            "jglc: Build scripts generated at {}",
            output_dir.join("build.sh").display()
        );
        return Ok(());
    }

    // Step 12: Run the build script
    println!("jglc: Bundle created successfully");
    println!("jglc: Running build script...");

    let build_sh = output_dir.join("build.sh");

    let result = std::process::Command::new("bash")
        .arg(&build_sh)
        .current_dir(output_dir)
        .status();
    match result {
        Ok(status) => {
            if status.success() {
                println!("jglc: Build completed successfully");
                Ok(())
            } else {
                Err(JglcError::Compiler(format!(
                    "Build script failed with exit code: {:?}",
                    status.code()
                )))
            }
        }
        Err(e) => Err(JglcError::Compiler(format!(
            "Failed to execute build script: {}",
            e
        ))),
    }
}

fn validate_project(project_path: &Path) -> Result<(), JglcError> {
    let content = fs::read_to_string(project_path).map_err(JglcError::Io)?;

    let jgl = parse_jgl(&content)?;

    // Check for manifest
    if jgl.manifest.is_none() {
        return Err(JglcError::Compiler(
            "No project._jgl manifest block found".to_string(),
        ));
    }

    let manifest = Manifest::parse(jgl.manifest.as_ref().unwrap())?;

    // Resolve imports to check for conflicts
    let base_dir = project_path.parent().unwrap_or(Path::new("."));
    let mut manifest = manifest;
    let imported_blocks = crate::manifest::resolve_imports(&mut manifest, base_dir)?;

    // Build VFS to check for conflicts
    let block_count = jgl.blocks.len() + imported_blocks.len();
    let mut all_blocks = jgl.blocks;
    all_blocks.extend(imported_blocks);
    let vfs = build_vfs(all_blocks, &manifest)?;

    // Validate dependency files
    let mut test_vfs = vfs;
    generate_dependency_files(&manifest, &manifest.custom_languages, &mut test_vfs)?;

    println!("jglc: Project '{}' is valid", project_path.display());
    println!("jglc: {} blocks", block_count);
    println!("jglc: {} custom languages", manifest.custom_languages.len());
    println!("jglc: {} dependency groups", manifest.dependencies.len());

    Ok(())
}

fn generate_build(
    project_path: &Path,
    output_dir: &Path,
    target_win: bool,
) -> Result<(), JglcError> {
    let content = fs::read_to_string(project_path).map_err(JglcError::Io)?;

    let jgl = parse_jgl(&content)?;

    let manifest = if let Some(manifest_block) = &jgl.manifest {
        Manifest::parse(manifest_block)?
    } else {
        return Err(JglcError::Compiler(
            "No project._jgl manifest block found".to_string(),
        ));
    };

    let base_dir = project_path.parent().unwrap_or(Path::new("."));
    let mut manifest = manifest;
    let imported_blocks = crate::manifest::resolve_imports(&mut manifest, base_dir)?;

    let mut all_blocks = jgl.blocks;
    all_blocks.extend(imported_blocks);
    let mut vfs = build_vfs(all_blocks, &manifest)?;
    generate_dependency_files(&manifest, &manifest.custom_languages, &mut vfs)?;

    if !output_dir.exists() {
        fs::create_dir_all(output_dir).map_err(JglcError::Io)?;
    }

    create_bundle(&vfs, output_dir)?;
    generate_build_sh(
        &manifest,
        &vfs,
        &manifest.custom_languages,
        output_dir,
        target_win,
    )?;
    // NOTE: Windows .bat generation removed - use --target-win for .exe cross-compilation

    println!("jglc: Build scripts generated at {}", output_dir.display());

    Ok(())
}
