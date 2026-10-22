use crate::error::JglcError;
use crate::parser::Block;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Compile target specification
#[derive(Debug, Clone)]
pub enum Target {
    Cross,
    Native,
    Triple(String),
}

/// Custom language definition
#[derive(Debug, Clone)]
pub struct CustomLanguage {
    /// The file extension this language handles (e.g., "vmx")
    pub extension: String,
    /// The compile command template
    pub compile_command: String,
    /// The dependency file name and template
    pub dep_file: String,
    /// Template for dependency entries (with {name} and {version} placeholders)
    pub dep_template: String,
}

/// Dependency declaration
#[derive(Debug, Clone)]
pub struct Manifest {
    pub target: Target,
    pub imports: Vec<PathBuf>,
    /// Custom languages defined in manifest
    pub custom_languages: Vec<CustomLanguage>,
    /// Dependencies per language: dep[lang] { name = "version" }
    pub dependencies: HashMap<String, Vec<DependencyEntry>>,
    /// Project/binary name (defaults to "jgl_binary" if not specified)
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct DependencyEntry {
    pub name: String,
    pub version: String,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            target: Target::Native,
            imports: Vec::new(),
            custom_languages: Vec::new(),
            dependencies: HashMap::new(),
            name: String::new(),
        }
    }
}

impl Manifest {
    pub fn parse(manifest_block: &Block) -> Result<Self, JglcError> {
        let mut manifest = Manifest::default();
        let lines: Vec<&str> = manifest_block.body.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with("//") {
                i += 1;
                continue;
            }

            // Parse import statements (keyword followed by space, not just prefix)
            if line.starts_with("import ") || line == "import" {
                let import = parse_import(line)?;
                manifest.imports.push(import);
                i += 1;
                continue;
            }

            // Parse target (keyword followed by space, not just prefix)
            if line.starts_with("target ") || line == "target" {
                let target = parse_target(line)?;
                manifest.target = target;
                i += 1;
                continue;
            }
            // Parse name (keyword followed by space)
            if line.starts_with("name ") {
                let name = line.strip_prefix("name").unwrap().trim();
                manifest.name = name.to_string();
                i += 1;
                continue;
            }
            // Parse custom language block: <extension>._sh {
            // Match lines like "vmx._sh {" — extract the extension before "._sh"
            if let Some(ext) = line.strip_suffix("._sh {") {
                let ext = ext.trim();
                if !ext.is_empty() {
                    i += 1;
                    {
                        let custom_lang = parse_custom_language(&lines, &mut i, ext)?;
                        manifest.custom_languages.push(custom_lang)
                    }
                    continue;
                }
            }

            // Parse dependency block: dep[lang] {
            if line.starts_with("dep[") && line.ends_with('{') {
                let lang = extract_dep_lang(line)?;
                i += 1;
                {
                    let entries = parse_dependency_block(&lines, &mut i)?;
                    manifest.dependencies.insert(lang, entries);
                }
                continue;
            }

            i += 1;
        }

        // Validate custom languages have dep blocks
        for _custom_lang in &manifest.custom_languages {
            // Custom languages must have a dep.<file> block (which is parsed inside the _sh block)
            // The parser already ensured this, but double check
        }

        Ok(manifest)
    }
}

fn parse_import(line: &str) -> Result<PathBuf, JglcError> {
    // Format: import "auth.jgl"
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(JglcError::Import(format!(
            "Invalid import statement: {}",
            line
        )));
    }

    let import_path = parts[1].trim().trim_matches('"').trim_matches('\'');
    if import_path.is_empty() {
        return Err(JglcError::Import(format!("Empty import path in: {}", line)));
    }

    Ok(PathBuf::from(import_path))
}

fn parse_target(line: &str) -> Result<Target, JglcError> {
    // Format: target cross
    //         target native
    //         target x86_64-unknown-linux-gnu
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(JglcError::Parse(format!(
            "Invalid target statement: {}",
            line
        )));
    }

    Ok(match parts[1] {
        "cross" => Target::Cross,
        "native" => Target::Native,
        triple => Target::Triple(triple.to_string()),
    })
}

fn parse_custom_language(
    lines: &[&str],
    i: &mut usize,
    ext: &str,
) -> Result<CustomLanguage, JglcError> {
    let mut compile_command = String::new();
    let mut dep_file = String::new();
    let mut dep_template = String::new();

    while *i < lines.len() {
        let line = lines[*i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            *i += 1;
            continue;
        }

        // Check for closing brace of the _sh block
        if line == "}" {
            *i += 1;
            break;
        }

        // Check for dep.<file> { block
        if line.starts_with("dep.") && line.ends_with('{') {
            let dep_header = line.trim_end_matches('{').trim();
            // Format: dep.filename
            if let Some(pos) = dep_header.find('.') {
                dep_file = dep_header[pos + 1..].trim().to_string();
            } else {
                // The whole thing after "dep" is the filename
                dep_file = dep_header
                    .trim_start_matches("dep")
                    .trim_start_matches('.')
                    .trim()
                    .to_string();
            }

            *i += 1;
            // Parse the template (single line)
            if *i < lines.len() {
                let template_line = lines[*i].trim();
                dep_template = template_line.trim().to_string();
            }
            *i += 1;

            // Skip until closing brace of dep block
            while *i < lines.len() && lines[*i].trim() != "}" {
                *i += 1;
            }
            if *i < lines.len() {
                *i += 1; // Skip the '}'
            }
            continue;
        }

        // Otherwise, it's the compile command
        if compile_command.is_empty() {
            compile_command = line.to_string();
        }

        *i += 1;
    }

    if dep_file.is_empty() {
        return Err(JglcError::CustomLanguageMissingDep(ext.to_string()));
    }

    Ok(CustomLanguage {
        extension: ext.to_string(),
        compile_command,
        dep_file,
        dep_template,
    })
}

fn parse_dependency_block(
    lines: &[&str],
    i: &mut usize,
) -> Result<Vec<DependencyEntry>, JglcError> {
    let mut entries = Vec::new();

    while *i < lines.len() {
        let line = lines[*i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            *i += 1;
            continue;
        }

        // Check for closing brace
        if line == "}" {
            *i += 1;
            break;
        }

        // Parse: name = "version"
        if let Some((name, version)) = parse_dependency_entry(line) {
            entries.push(DependencyEntry { name, version });
        } else {
            return Err(JglcError::Parse(format!(
                "Invalid dependency entry: {}",
                line
            )));
        }

        *i += 1;
    }

    Ok(entries)
}

fn parse_dependency_entry(line: &str) -> Option<(String, String)> {
    // Format: name = "version"
    let eq_pos = line.find('=')?;
    let name = line[..eq_pos].trim().to_string();
    let version_part = line[eq_pos + 1..].trim();
    let version = version_part
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();

    if name.is_empty() {
        return None;
    }

    Some((name, version))
}

fn extract_dep_lang(line: &str) -> Result<String, JglcError> {
    // Format: dep[lang] {
    let start = line
        .find('[')
        .ok_or_else(|| JglcError::Parse(format!("Invalid dep block: {}", line)))?;
    let end = line
        .find(']')
        .ok_or_else(|| JglcError::Parse(format!("Invalid dep block: {}", line)))?;
    let lang = line[start + 1..end].trim().to_string();

    if lang.is_empty() {
        return Err(JglcError::Parse(format!(
            "Empty language in dep block: {}",
            line
        )));
    }

    Ok(lang)
}

/// Resolve imports and return additional blocks from imported files
pub fn resolve_imports(
    manifest: &mut Manifest,
    base_dir: &Path,
) -> Result<Vec<crate::parser::Block>, JglcError> {
    resolve_imports_inner(manifest, base_dir, &mut HashSet::new())
}

/// Internal recursive implementation with cycle detection
fn resolve_imports_inner(
    manifest: &mut Manifest,
    base_dir: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<Vec<crate::parser::Block>, JglcError> {
    let mut imported_blocks: Vec<crate::parser::Block> = Vec::new();

    for import in &manifest.imports {
        let import_path = if import.is_absolute() {
            import.clone()
        } else {
            base_dir.join(import)
        };

        let canonical = import_path.canonicalize().unwrap_or(import_path.clone());
        if visited.contains(&canonical) {
            return Err(JglcError::Import(format!(
                "Circular import detected: {}",
                import_path.display()
            )));
        }
        visited.insert(canonical.clone());

        let import_path = if import.is_absolute() {
            import.clone()
        } else {
            base_dir.join(import)
        };

        if !import_path.exists() {
            return Err(JglcError::Import(format!(
                "Imported file not found: {}",
                import_path.display()
            )));
        }

        let content = std::fs::read_to_string(&import_path).map_err(|e| {
            JglcError::Import(format!(
                "Failed to read import {}: {}",
                import_path.display(),
                e
            ))
        })?;

        let imported = crate::parser::parse_jgl(&content)?;

        if let Some(imported_manifest) = imported.manifest {
            let mut sub_manifest = Manifest::parse(&imported_manifest)?;
            // Depth-first resolution: recursively resolve sub-imports
            let sub_imports = resolve_imports_inner(
                &mut sub_manifest,
                import_path.parent().unwrap_or(base_dir),
                visited,
            )?;
            imported_blocks.extend(sub_imports);

            // Merge dependencies
            for (lang, deps) in sub_manifest.dependencies {
                let existing = manifest.dependencies.entry(lang.clone()).or_default();
                merge_dependencies(existing, &deps)?;
                // Check if empty after merge (shouldn't happen since we add entries)
            }

            // Merge custom languages
            manifest
                .custom_languages
                .extend(sub_manifest.custom_languages);
        }

        // Collect blocks from this import
        imported_blocks.extend(imported.blocks);
    }

    Ok(imported_blocks)
}

/// Merge dependency entries, deduplicating exact matches, erroring on version conflicts
fn merge_dependencies(
    existing: &mut Vec<DependencyEntry>,
    new_entries: &[DependencyEntry],
) -> Result<(), JglcError> {
    for entry in new_entries {
        let found = existing.iter().find(|e| e.name == entry.name);
        match found {
            Some(existing_entry) => {
                if existing_entry.version != entry.version {
                    return Err(JglcError::DepsConflict(format!(
                        "Dependency '{}' has conflicting versions: '{}' vs '{}'",
                        entry.name, existing_entry.version, entry.version
                    )));
                }
                // Same name and version - deduplicate (skip)
            }
            None => {
                existing.push(entry.clone());
            }
        }
    }
    Ok(())
}

/// Check if a language can have dependencies
pub fn language_supports_deps(lang: &str) -> bool {
    !matches!(lang, "sh" | "md" | "adoc" | "bs")
}
