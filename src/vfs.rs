use crate::error::JglcError;
use crate::parser::Block;
use crate::manifest::Manifest;
use std::collections::HashMap;

/// A file in the virtual filesystem
#[derive(Debug, Clone)]
pub struct VfsEntry {
    /// The path relative to VFS root, e.g., "main.rs", "v1.lua"
    pub path: String,
    /// The file content
    pub content: String,
    /// The language extension
    pub language: String,
    /// Whether this is a manifest file (only project._jgl is kept)
    pub is_manifest: bool,
    /// Whether this is a custom language file
    pub is_custom: bool,
    /// Whether this is a data file (md, adoc)
    pub is_data: bool,
}

/// The virtual filesystem
#[derive(Debug, Default)]
pub struct Vfs {
    /// All files in the VFS, keyed by path
    pub files: HashMap<String, VfsEntry>,
    /// Files in declaration order
    pub ordered_files: Vec<String>,
}

impl Vfs {
    pub fn new() -> Self {
        Vfs {
            files: HashMap::new(),
            ordered_files: Vec::new(),
        }
    }

    pub fn add_file(&mut self, path: String, entry: VfsEntry) -> Result<(), JglcError> {
        if self.files.contains_key(&path) {
            return Err(JglcError::Conflict(format!(
                "Named block conflict: file '{}' already exists in VFS",
                path
            )));
        }
        self.files.insert(path.clone(), entry);
        self.ordered_files.push(path);
        Ok(())
    }

    pub fn get(&self, path: &str) -> Option<&VfsEntry> {
        self.files.get(path)
    }

    pub fn list(&self) -> Vec<&VfsEntry> {
        self.ordered_files.iter()
            .filter_map(|path| self.files.get(path))
            .collect()
    }

    /// Check if a path exists in the VFS
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}

/// Build the virtual filesystem from parsed blocks and manifest
pub fn build_vfs(
    blocks: Vec<Block>,
    manifest: &Manifest,
) -> Result<Vfs, JglcError> {
    let mut vfs = Vfs::new();

    // Custom language extensions
    let custom_extensions: std::collections::HashSet<String> = manifest
        .custom_languages
        .iter()
        .map(|cl| cl.extension.clone())
        .collect();

    // Data file extensions (not compiled, just embedded as static data)
    let data_extensions: std::collections::HashSet<&str> = 
        ["md", "adoc"].iter().map(|&s| s).collect();

    // Process each block
    for block in blocks {
        let filename = if block.is_manifest {
            if block.is_project_manifest {
                // The project manifest is kept as project._jgl
                continue; // Manifest is not a VFS file but handled separately
            } else {
                continue; // Non-project _jgl blocks are discarded
            }
        } else if block.is_named {
            // Named block: name.extension
            format!("{}.{}", block.name, block.language)
        } else {
            // Unnamed block: vX.extension
            format!("v{}.{}", block.name, block.language)
        };

        // Determine file properties
        let language = block.language.clone();
        let is_custom = custom_extensions.contains(&language);
        let is_data = data_extensions.contains(language.as_str());

        // Generate lint suppressors (per spec, section 5)
        let mut content = generate_lint_suppressor(&language);
        let body = dedent(&block.body);
        if !body.is_empty() {
            if !content.is_empty() {
                content.push('\n');
            }
            content.push_str(&body);
        }

        let entry = VfsEntry {
            path: filename.clone(),
            content,
            language,
            is_manifest: false,
            is_custom,
            is_data,
        };

        vfs.add_file(filename, entry)?;
    }

    Ok(vfs)
}

/// Generate the appropriate lint/style suppressor for each language
fn generate_lint_suppressor(lang: &str) -> String {
    match lang {
        "rs" => "#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]",
        "java" => "@SuppressWarnings(\"all\")",
        "kt" => "@Suppress(\"all\")",
        "cs" => "#pragma warning disable",
        "js" => "/* eslint-disable */",
        "ts" => "// @ts-nocheck",
        "jsx" => "/* eslint-disable */\n// @ts-nocheck",
        "tsx" => "/* eslint-disable */\n// @ts-nocheck",
        "go" => "//nolint:all",
        "py" => "# noqa: ALL",
        "c" | "cpp" => "#pragma GCC diagnostic ignored \"-Wall\"",
        "rb" => "# rubocop:disable all",
        "php" => "// phpcs:ignoreFile",
        "scala" => "//scalastyle:off",
        "lua" => "",
        "md" => "",
        "adoc" => "",
        "bs" => "",
        "zig" => "",
        "nim" => "",
        "f90" => "",
        _ => "",
    }
    .to_string()
}
/// Strip common leading whitespace from each line of a string.
/// This handles blocks indented inside `.jgl` braces so that indentation-sensitive
/// languages like Nim, Python, YAML, etc. parse correctly.
fn dedent(body: &str) -> String {
    // Trim leading/trailing blank lines, then find common indentation
    let lines: Vec<&str> = body.lines().collect();

    // Find first non-empty line to determine the indentation to strip
    let first_non_empty = lines.iter().position(|l| !l.trim().is_empty());
    let first_idx = match first_non_empty {
        Some(idx) => idx,
        None => return body.trim().to_string(),
    };

    // Calculate common leading whitespace from all non-empty lines
    let first_indent = lines[first_idx].chars().take_while(|c| *c == ' ' || *c == '\t').count();
    let common_indent = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take_while(|c| *c == ' ' || *c == '\t').count())
        .min()
        .unwrap_or(0)
        .min(first_indent);

    // Strip the common indentation from each line
    let result: String = lines.iter().map(|l| {
        if l.len() >= common_indent {
            &l[common_indent..]
        } else {
            l.trim_start()
        }
    }).collect::<Vec<&str>>().join("\n");

    result.trim().to_string()
}
