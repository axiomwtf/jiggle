use crate::build_scripts::generate_build_sh;
use crate::bundler::create_bundle;
use crate::cli::Commands;
use crate::deps::generate_dependency_files;
use crate::error::JglcError;
use crate::manifest::Manifest;
use crate::parser::parse_jgl;
use crate::vfs::{Vfs, build_vfs};
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
        } => {
            write_build(&project, &output, target_win)?;
            println!("jglc: Build scripts generated at {}", output.display());
            Ok(())
        }
    }
}

/// A project read from disk: its manifest, its VFS with the generated dep
/// files, and the number of blocks it was built from.
struct Project {
    manifest: Manifest,
    vfs: Vfs,
    block_count: usize,
}

/// Read project.jgl, resolve its imports and build the VFS.
fn load_project(project_path: &Path) -> Result<Project, JglcError> {
    let content = fs::read_to_string(project_path).map_err(JglcError::Io)?;
    let jgl = parse_jgl(&content)?;

    let manifest_block = jgl
        .manifest
        .as_ref()
        .ok_or_else(|| JglcError::Compiler("No project._jgl manifest block found".to_string()))?;
    let mut manifest = Manifest::parse(manifest_block)?;
    let imported_blocks = crate::manifest::resolve_imports(&mut manifest, project_path)?;

    let block_count = jgl.blocks.len() + imported_blocks.len();
    let mut all_blocks = jgl.blocks;
    all_blocks.extend(imported_blocks);
    let mut vfs = build_vfs(all_blocks, &manifest)?;
    generate_dependency_files(&manifest, &manifest.custom_languages, &mut vfs)?;

    Ok(Project {
        manifest,
        vfs,
        block_count,
    })
}

/// Load the project and write project.tar.xz and build.sh to `output_dir`.
fn write_build(project_path: &Path, output_dir: &Path, target_win: bool) -> Result<(), JglcError> {
    let project = load_project(project_path)?;

    fs::create_dir_all(output_dir).map_err(JglcError::Io)?;
    create_bundle(&project.vfs, output_dir)?;
    generate_build_sh(
        &project.manifest,
        &project.vfs,
        &project.manifest.custom_languages,
        output_dir,
        target_win,
    )
}

fn compile_project(
    project_path: &Path,
    output_dir: &Path,
    bundle_only: bool,
    target_win: bool,
) -> Result<(), JglcError> {
    write_build(project_path, output_dir, target_win)?;

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

    println!("jglc: Bundle created successfully");
    println!("jglc: Running build script...");

    let status = std::process::Command::new("bash")
        .arg("build.sh")
        .current_dir(output_dir)
        .status()
        .map_err(|e| JglcError::Compiler(format!("Failed to execute build script: {}", e)))?;
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

fn validate_project(project_path: &Path) -> Result<(), JglcError> {
    let project = load_project(project_path)?;

    println!("jglc: Project '{}' is valid", project_path.display());
    println!("jglc: {} blocks", project.block_count);
    println!(
        "jglc: {} custom languages",
        project.manifest.custom_languages.len()
    );
    println!(
        "jglc: {} dependency groups",
        project.manifest.dependencies.len()
    );

    Ok(())
}
