use crate::error::JglcError;
use crate::manifest::Manifest;
use crate::parser::Block;
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
        self.ordered_files
            .iter()
            .filter_map(|path| self.files.get(path))
            .collect()
    }

    /// Check if a path exists in the VFS
    pub fn contains(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
}

/// Build the virtual filesystem from parsed blocks and manifest
pub fn build_vfs(blocks: Vec<Block>, manifest: &Manifest) -> Result<Vfs, JglcError> {
    let mut vfs = Vfs::new();

    // Custom language extensions
    let custom_extensions: std::collections::HashSet<String> = manifest
        .custom_languages
        .iter()
        .map(|cl| cl.extension.clone())
        .collect();

    // Data file extensions (not compiled, just embedded as static data)
    let data_extensions: std::collections::HashSet<&str> = ["md", "adoc"].iter().copied().collect();

    // Process each block
    for block in blocks {
        let mut filename = if block.is_manifest {
            if block.is_project_manifest {
                // The project manifest is kept as project._jgl
                continue; // Manifest is not a VFS file but handled separately
            } else {
                continue; // Non-project ._jgl blocks are discarded
            }
        } else if block.is_named {
            if !crate::manifest::is_safe_name(&block.name)
                || !crate::manifest::is_safe_extension(&block.language)
            {
                return Err(JglcError::BlockSyntax(format!(
                    "'{}': block names take letters, digits, '_', '-' and '.' (not starting with '.' or '-'), languages take letters, digits and '_'",
                    block.header
                )));
            }
            // Named block: name.extension
            format!("{}.{}", block.name, block.language)
        } else {
            // Unnamed block: vX.extension
            format!("v{}.{}", block.name, block.language)
        };

        // A Java file has to be named after the class it declares, so that
        // javac accepts a public class. A file may hold at most one public
        // type, so a block that declares two of them is refused here
        // rather than by the compiler of Java.
        let body = dedent(&block.body, &block.language);
        if block.language == "java" {
            let names = public_class_names(&body);
            if names.len() > 1 {
                return Err(JglcError::Compiler(format!(
                    "The Java block declares more than one public type (\"{}\" and \"{}\"). \
                     A Java file may hold at most one public type, so each of them needs a \
                     block of its own.",
                    names[0], names[1]
                )));
            }
            if let Some(class_name) = names.first() {
                filename = format!("{}.java", class_name);
            }
        }

        // Determine file properties
        let language = block.language.clone();
        let is_custom = custom_extensions.contains(&language);
        let is_data = data_extensions.contains(language.as_str());

        // Generate lint suppressors (per spec, section 5)
        let mut content = generate_lint_suppressor(&language);
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

/// The public types a Java source declares. A Java file has to be named
/// after the public type it contains, otherwise javac rejects it, and a
/// file may hold at most one public type. The type may be a class, an enum,
/// an interface or a record. Only the types at the top level count: a
/// public type nested in another one does not need the file to be named
/// after it.
fn public_class_names(source: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(
        r"(?m)^public\s+(?:final\s+|abstract\s+|sealed\s+|non-sealed\s+|static\s+)*(?:class|enum|interface|record)\s+([A-Za-z_][A-Za-z0-9_]*)",
    ) else {
        return Vec::new();
    };
    re.captures_iter(source)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Generate the appropriate lint/style suppressor for each language
fn generate_lint_suppressor(lang: &str) -> String {
    match lang {
        "rs" => "#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]",
        // Java and Kotlin file annotations have to follow the package
        // declaration, so they are not put in front of the file. Their
        // compilers are told to stay quiet instead.
        "java" => "",
        "kt" => "",
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
/// The source with the text of the strings and of the comments blanked out.
///
/// The blanks keep the length of the source, so that an offset in the
/// masked source is the offset in the source itself. A definition that is
/// written in a docstring is then not read as a definition.
pub(crate) fn mask_python_code(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '#' {
            // A comment runs to the end of the line.
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c != '"' && c != '\'' {
            out.push(c);
            i += 1;
            continue;
        }
        let quote = c;
        // A string of three quotes may hold a line break.
        let triple = i + 2 < chars.len() && chars[i + 1] == quote && chars[i + 2] == quote;
        let open = if triple { 3 } else { 1 };
        for _ in 0..open {
            out.push(quote);
            i += 1;
        }
        loop {
            if i >= chars.len() {
                break;
            }
            if !triple && chars[i] == '\\' {
                out.push(' ');
                i += 1;
                if i < chars.len() {
                    if chars[i] == '\n' {
                        out.push('\n');
                    } else {
                        out.push(' ');
                    }
                    i += 1;
                }
                continue;
            }
            if triple {
                if i + 2 < chars.len()
                    && chars[i] == quote
                    && chars[i + 1] == quote
                    && chars[i + 2] == quote
                {
                    // The string of three quotes is one statement, so the
                    // lines it holds are blanked out as well.
                    for _ in 0..3 {
                        out.push(' ');
                    }
                    i += 3;
                    break;
                }
            } else if chars[i] == quote {
                out.push(quote);
                i += 1;
                break;
            }
            if chars[i] == '\n' {
                out.push('\n');
            } else {
                out.push(' ');
            }
            i += 1;
        }
    }
    out
}

/// Push as many blanks as the character takes bytes, so that the masked
/// source is as long as the source itself.
fn push_blank(out: &mut String, c: char) {
    for _ in 0..c.len_utf8() {
        out.push(' ');
    }
}

/// The source with the text of the strings and of the comments blanked out,
/// for the languages whose comments start with "//" and whose strings are
/// written with quotes (C, C++, C#, Java, Kotlin, Scala, JavaScript,
/// TypeScript, Go, Rust, ...).
///
/// The blanks keep the length of the source and the line structure, so
/// that an offset in the masked source is the offset in the source itself.
///
/// The quote that opens a string is kept, so that the line that opens it
/// is still read as code. The quote that closes a string is blanked when
/// the string runs over a line break, so that the line that closes it is
/// not read as code. A quote of a character literal is kept, unless the
/// literal is not closed on its line: such a quote is the mark of a
/// lifetime of Rust, as in `&'a str`, and it opens no string.
pub(crate) fn mask_c_style_code(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            // A comment of a line runs to the end of the line.
            while i < chars.len() && chars[i] != '\n' {
                push_blank(&mut out, chars[i]);
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // A comment of a block runs to the matching "*/".
            push_blank(&mut out, chars[i]);
            push_blank(&mut out, chars[i + 1]);
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    push_blank(&mut out, chars[i]);
                    push_blank(&mut out, chars[i + 1]);
                    i += 2;
                    break;
                }
                if chars[i] == '\n' {
                    out.push('\n');
                } else {
                    push_blank(&mut out, chars[i]);
                }
                i += 1;
            }
            continue;
        }
        if c == '"' || c == '`' || c == '\'' {
            // The quote opens a string, unless the string is not closed on
            // its line: such a quote is the mark of a lifetime of Rust.
            let closed = closing_quote(&chars, i, c);
            if closed.is_none() && c == '\'' {
                out.push(c);
                i += 1;
                continue;
            }
            // The quote that opens the string is kept, so that the line
            // that opens it is still read as code.
            out.push(c);
            i += 1;
            let mut over_line = false;
            while i < chars.len() {
                let d = chars[i];
                if d == '\\' {
                    push_blank(&mut out, d);
                    i += 1;
                    if i < chars.len() {
                        if chars[i] == '\n' {
                            out.push('\n');
                            over_line = true;
                        } else {
                            push_blank(&mut out, chars[i]);
                        }
                        i += 1;
                    }
                    continue;
                }
                if d == c {
                    // The quote that closes the string is blanked when the
                    // string runs over a line break, so that the line that
                    // closes it is not read as code.
                    if over_line {
                        push_blank(&mut out, d);
                    } else {
                        out.push(d);
                    }
                    i += 1;
                    break;
                }
                if d == '\n' {
                    out.push('\n');
                    over_line = true;
                } else {
                    push_blank(&mut out, d);
                }
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The place of the quote that closes the string that opens at `i`, when
/// the string is closed on its line. A backslash escapes the next
/// character.
fn closing_quote(chars: &[char], i: usize, quote: char) -> Option<usize> {
    let mut j = i + 1;
    while j < chars.len() {
        if chars[j] == '\n' {
            return None;
        }
        if chars[j] == '\\' {
            j += 2;
            continue;
        }
        if chars[j] == quote {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// The source with the strings and the comments blanked out, for the
/// language of the block. A source whose language is not known is left as
/// it is.
fn mask_source(body: &str, language: &str) -> String {
    match language {
        "py" => mask_python_code(body),
        "c" | "cpp" | "cxx" | "cs" | "java" | "kt" | "scala" | "js" | "jsx" | "ts" | "tsx"
        | "rs" | "go" | "zig" | "nim" | "swift" | "c#" => mask_c_style_code(body),
        _ => body.to_string(),
    }
}

/// The number of leading blanks of a line.
fn line_indent(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// Strip common leading whitespace from each line of a string.
/// This handles blocks indented inside `.jgl` braces so that indentation-sensitive
/// languages like Nim, Python, YAML, etc. parse correctly.
///
/// The common indentation is computed over the lines that hold code, so
/// that a line that lies inside a string or inside a comment does not
/// hold it down. The indentation is stripped from the source itself, so
/// that the strings and the comments are emitted as they were written.
fn dedent(body: &str, language: &str) -> String {
    // Trim leading/trailing blank lines, then find common indentation
    let lines: Vec<&str> = body.lines().collect();
    let masked = mask_source(body, language);
    let masked_lines: Vec<&str> = masked.lines().collect();
    // The masked source has the same lines as the source itself. Should it
    // not, the source is used as it is.
    let masked_lines: Vec<&str> = if masked_lines.len() == lines.len() {
        masked_lines
    } else {
        lines.clone()
    };

    // A line whose text lies inside a string or inside a comment is
    // masked out: it holds no code, so it does not count.
    let holds_code: Vec<bool> = masked_lines.iter().map(|l| !l.trim().is_empty()).collect();

    // Find first non-empty line to determine the indentation to strip
    let first_idx = match holds_code.iter().position(|c| *c) {
        Some(idx) => idx,
        None => return body.trim().to_string(),
    };

    // Calculate common leading whitespace from all non-empty lines
    let first_indent = line_indent(lines[first_idx]);
    let common_indent = holds_code
        .iter()
        .enumerate()
        .filter(|(_, c)| **c)
        .map(|(i, _)| line_indent(lines[i]))
        .min()
        .unwrap_or(0)
        .min(first_indent);
    let result: String = lines
        .iter()
        .map(|l| {
            let mut stripped = 0usize;
            let mut end = 0usize;
            for (i, c) in l.char_indices() {
                if stripped >= common_indent {
                    break;
                }
                if c != ' ' && c != '\t' {
                    break;
                }
                stripped += 1;
                end = i + c.len_utf8();
            }
            &l[end..]
        })
        .collect::<Vec<&str>>()
        .join("\n");

    result.trim().to_string()
}
