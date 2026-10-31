use crate::error::JglcError;
use crate::manifest::{CustomLanguage, DependencyEntry, Manifest};
use crate::vfs::Vfs;
use std::collections::HashMap;

/// Generate native dependency files and add them to the VFS
pub fn generate_dependency_files(
    manifest: &Manifest,
    custom_languages: &[CustomLanguage],
    vfs: &mut Vfs,
) -> Result<(), JglcError> {
    // Validate dependency declarations
    for lang in manifest.dependencies.keys() {
        if !crate::manifest::language_supports_deps(lang) {
            return Err(JglcError::Compiler(format!(
                "Language '{}' does not support dependency declarations (no ecosystem file)",
                lang
            )));
        }
    }

    // Generate dep files for each language
    // Languages that share the same dependency file (e.g. js/ts/tsx/jsx -> package.json,
    // java/kt/scala -> pom.xml, c/cpp -> vcpkg.json) are merged into a single file
    // to avoid collisions
    let mut files_by_output: HashMap<String, Vec<DependencyEntry>> = HashMap::new();
    let mut lang_per_output: HashMap<String, String> = HashMap::new();
    for (lang, entries) in &manifest.dependencies {
        let dep_filename = get_dep_filename(lang, custom_languages)?;
        let existing = files_by_output.entry(dep_filename.clone()).or_default();
        for entry in entries {
            // Avoid duplicate entries (same name)
            if !existing
                .iter()
                .any(|e: &DependencyEntry| e.name == entry.name)
            {
                existing.push(entry.clone());
            }
        }
        lang_per_output
            .entry(dep_filename)
            .or_insert_with(|| lang.clone());
    }
    for (dep_filename, entries) in &files_by_output {
        let Some(lang) = lang_per_output.get(dep_filename) else {
            continue;
        };
        let dep_file_content = generate_dep_file(lang, entries, custom_languages)?;

        // Check for collision with existing VFS file
        if vfs.contains(dep_filename) {
            return Err(JglcError::DepFileNameCollision(format!(
                "Generated dependency file '{}' conflicts with an existing VFS file",
                dep_filename
            )));
        }

        let entry = crate::vfs::VfsEntry {
            path: dep_filename.clone(),
            content: dep_file_content,
            language: lang.clone(),
            is_manifest: false,
            is_custom: false,
            is_data: false,
        };

        vfs.add_file(dep_filename.clone(), entry)?;
    }

    Ok(())
}

/// Determine the dependency file name for a language
pub fn get_dep_filename(
    lang: &str,
    custom_languages: &[CustomLanguage],
) -> Result<String, JglcError> {
    match lang {
        "rs" => Ok("Cargo.toml".to_string()),
        "js" | "ts" | "tsx" | "jsx" => Ok("package.json".to_string()),
        "go" => Ok("go.mod".to_string()),
        "py" => Ok("requirements.txt".to_string()),
        "java" | "kt" | "scala" => Ok("pom.xml".to_string()),
        "cs" => Ok("packages.config".to_string()),
        "rb" => Ok("Gemfile".to_string()),
        "php" => Ok("composer.json".to_string()),
        "zig" => Ok("build.zig.zon".to_string()),
        "nim" => Ok("jgl.nimble".to_string()),
        "lua" => Ok("jgl-scm-1.rockspec".to_string()),
        "c" | "cpp" => Ok("vcpkg.json".to_string()),
        "f90" => Ok("fpm.toml".to_string()),
        _ => {
            // Check if it's a custom language
            if let Some(custom) = custom_languages.iter().find(|cl| cl.extension == lang) {
                Ok(custom.dep_file.clone())
            } else {
                Err(JglcError::Compiler(format!(
                    "Unknown language for dependency file: {}",
                    lang
                )))
            }
        }
    }
}

/// Generate the dependency file content for a language
fn generate_dep_file(
    lang: &str,
    entries: &[DependencyEntry],
    custom_languages: &[CustomLanguage],
) -> Result<String, JglcError> {
    match lang {
        "rs" => generate_cargo_toml(entries),
        "js" | "ts" | "tsx" | "jsx" => generate_package_json(entries),
        "go" => generate_go_mod(entries),
        "py" => generate_requirements_txt(entries),
        "java" | "kt" | "scala" => generate_pom_xml(entries),
        "cs" => generate_packages_config(entries),
        "rb" => generate_gemfile(entries),
        "php" => generate_composer_json(entries),
        "zig" => generate_build_zig_zon(entries),
        "nim" => generate_nimble(entries),
        "lua" => generate_rockspec(entries),
        "c" | "cpp" => generate_vcpkg_json(entries),
        "f90" => generate_fpm_toml(entries),
        _ => {
            // For custom languages, use the template
            if let Some(custom) = custom_languages.iter().find(|cl| cl.extension == lang) {
                Ok(generate_custom_dep_file(entries, custom))
            } else {
                Err(JglcError::Compiler(format!(
                    "No dependency file generator for language: {}",
                    lang
                )))
            }
        }
    }
}

fn generate_custom_dep_file(entries: &[DependencyEntry], custom: &CustomLanguage) -> String {
    let mut lines = Vec::new();

    for entry in entries {
        let line = custom
            .dep_template
            .replace("{name}", &entry.name)
            .replace("{version}", &entry.version);
        lines.push(line);
    }

    format!("{}\n", lines.join("\n"))
}

fn generate_cargo_toml(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    // Generate Cargo.toml with a dummy lib target so cargo build resolves deps
    // The actual source files are compiled separately to .o for unified linking
    let mut content = String::from(
        "[package]\nname = \"jgl-project\"\nversion = \"0.1.0\"\nedition = \"2024\"\nautobins = false\nautoexamples = false\nautotests = false\nautobenches = false\n\n[lib]\nname = \"jgl_project\"\npath = \"jgl_dummy.rs\"\n\n[dependencies]\n",
    );

    for entry in entries {
        content.push_str(&format!("{} = \"{}\"\n", entry.name, entry.version));
    }

    Ok(content)
}

fn generate_package_json(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(
        "{\n  \"name\": \"jgl-project\",\n  \"version\": \"0.1.0\",\n  \"dependencies\": {\n",
    );

    let mut first = true;
    for entry in entries {
        if !first {
            content.push_str(",\n");
        }
        first = false;
        content.push_str(&format!("    \"{}\": \"{}\"", entry.name, entry.version));
    }

    content.push_str("\n  }\n}\n");
    Ok(content)
}

fn generate_go_mod(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from("module jgl\n\ngo 1.21\n\n");

    if !entries.is_empty() {
        content.push_str("require (\n");
        for entry in entries {
            // Go requires full module path: github.com/user/module v1.2.3
            // If entry.name doesn't contain a slash, prefix with common paths
            let module_path = if entry.name.contains('/') {
                entry.name.clone()
            } else {
                // Map common Go module names to their full paths
                match entry.name.as_str() {
                    "gin-gonic" => "github.com/gin-gonic/gin".to_string(),
                    "gorm" => "gorm.io/gorm".to_string(),
                    "gofiber" => "github.com/gofiber/fiber/v2".to_string(),
                    "echo" => "github.com/labstack/echo/v4".to_string(),
                    "gorilla" => "github.com/gorilla/mux".to_string(),
                    _ => format!("github.com/{}", entry.name),
                }
            };
            let version = if entry.version.starts_with('v') || entry.version.starts_with('V') {
                entry.version.clone()
            } else {
                format!("v{}", entry.version)
            };
            content.push_str(&format!("    {} {}\n", module_path, version));
        }
        content.push_str(")\n");
    }

    Ok(content)
}

fn generate_requirements_txt(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::new();

    for entry in entries {
        if entry.version == "*" {
            content.push_str(&format!("{}\n", entry.name));
        } else if entry.version.starts_with('>')
            || entry.version.starts_with('<')
            || entry.version.starts_with('=')
            || entry.version.starts_with('!')
        {
            // Version specifiers like >=1.0, <2.0, ==1.0, !=1.0
            content.push_str(&format!("{}{}\n", entry.name, entry.version));
        } else if entry.version.starts_with('~') || entry.version.starts_with('^') {
            // Compatible version specifiers
            content.push_str(&format!("{}{}\n", entry.name, entry.version));
        } else {
            // Exact version
            content.push_str(&format!("{}=={}\n", entry.name, entry.version));
        }
    }

    Ok(content)
}

fn generate_pom_xml(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <project xmlns=\"http://maven.apache.org/POM/4.0.0\">\n\
        <modelVersion>4.0.0</modelVersion>\n\
        <groupId>jgl</groupId>\n\
        <artifactId>jgl-project</artifactId>\n\
        <version>0.1.0</version>\n\
        <dependencies>\n",
    );

    for entry in entries {
        // Java/Kotlin/Scala use group:artifact as the name
        let parts: Vec<&str> = entry.name.splitn(2, ':').collect();
        let group = parts.first().copied().unwrap_or("jgl");
        let artifact = if parts.len() == 2 {
            parts[1]
        } else {
            entry.name.as_str()
        };

        content.push_str(&format!(
            "    <dependency>\n      <groupId>{}</groupId>\n      <artifactId>{}</artifactId>\n      <version>{}</version>\n    </dependency>\n",
            group,
            artifact,
            xml_escape(&entry.version)
        ));
    }

    content.push_str("</dependencies>\n</project>\n");
    Ok(content)
}

fn generate_packages_config(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<packages>\n");

    for entry in entries {
        content.push_str(&format!(
            "  <package id=\"{}\" version=\"{}\" targetFramework=\"native\" />\n",
            entry.name,
            xml_escape(&entry.version)
        ));
    }

    content.push_str("</packages>\n");
    Ok(content)
}

fn generate_gemfile(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content =
        String::from("# frozen_string_literal: true\n\nsource 'https://rubygems.org'\n\n");

    for entry in entries {
        if entry.version == "*" {
            content.push_str(&format!("gem '{}'\n", entry.name));
        } else {
            content.push_str(&format!("gem '{}', '{}'\n", entry.name, entry.version));
        }
    }

    Ok(content)
}

fn generate_composer_json(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from("{\n  \"name\": \"jgl/project\",\n  \"require\": {\n");

    let mut first = true;
    for entry in entries {
        if !first {
            content.push_str(",\n");
        }
        first = false;
        let version = if entry.version == "*" {
            entry.version.clone()
        } else {
            format!("^{}", entry.version.trim_start_matches('^'))
        };
        content.push_str(&format!("    \"{}\": \"{}\"", entry.name, version));
    }

    content.push_str("\n  }\n}\n");
    Ok(content)
}

fn generate_build_zig_zon(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(".{\n");

    for entry in entries {
        let url = if entry.version == "*" {
            format!("https://github.com/{}/{}.git", entry.name, entry.name)
        } else {
            format!(
                "https://github.com/{}/{}/archive/{}.tar.gz",
                entry.name, entry.name, entry.version
            )
        };
        content.push_str(&format!(
            "    \"{}\" = .{{\n        .url = \"{}\",\n        // Run `zig fetch` to generate the correct .hash\n        .hash = \"0000000000000000000000000000000000000000000000000000000000000000\",\n    }},\n",
            entry.name, url
        ));
    }

    content.push_str("}\n");
    Ok(content)
}

fn generate_nimble(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(
        "# Package\nversion       = \"0.1.0\"\nauthor        = \"Jiggle\"\ndescription = \"JGL project\"\n\n",
    );

    for entry in entries {
        content.push_str(&format!("requires \"{}\"\n", entry.name));
    }

    Ok(content)
}

fn generate_rockspec(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(
        "rockspec_format = \"3.0\"\n\
        package = \"jgl-project\"\n\
        version = \"0.1.0-1\"\n\n\
        dependencies = {\n",
    );

    for entry in entries {
        content.push_str(&format!("  \"{}\",\n", entry.name));
    }

    content.push_str("}\n");
    Ok(content)
}

fn generate_vcpkg_json(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content = String::from(
        "{\n  \"name\": \"jgl-project\",\n  \"version-string\": \"0.1.0\",\n  \"dependencies\": [\n",
    );

    let mut first = true;
    for entry in entries {
        if !first {
            content.push_str(", ");
        }
        first = false;
        content.push_str(&format!("\"{}\"", entry.name));
    }

    content.push_str("\n  ]\n}\n");
    Ok(content)
}

fn generate_fpm_toml(entries: &[DependencyEntry]) -> Result<String, JglcError> {
    let mut content =
        String::from("[project]\nname = \"jgl-project\"\nversion = \"0.1.0\"\n\n[dependencies]\n");

    for entry in entries {
        content.push_str(&format!("{} = \"{}\"\n", entry.name, entry.version));
    }

    Ok(content)
}

/// Escape the characters XML gives a meaning to. Names never hold them;
/// versions may (for example `<2.0`).
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
