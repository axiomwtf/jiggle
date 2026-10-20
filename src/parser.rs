use crate::error::JglcError;
use std::collections::HashMap;

/// A block parsed from a .jgl file
#[derive(Debug, Clone)]
pub struct Block {
    /// The full header name, e.g., "main._rs", ".js", "project._jgl"
    pub header: String,
    /// The block name (without language sigil), e.g., "main", "v1", "project"
    pub name: String,
    /// The language extension, e.g., "rs", "js", "jgl"
    pub language: String,
    /// Whether this is a named block (has a name before the ._)
    pub is_named: bool,
    /// The content of the block body
    pub body: String,
    /// Whether this is the manifest block (._jgl)
    pub is_manifest: bool,
    /// Whether this is the "project" manifest block
    pub is_project_manifest: bool,
}

/// Parsed structure of a .jgl project file
#[derive(Debug, Default)]
pub struct JglFile {
    pub blocks: Vec<Block>,
    pub manifest: Option<Block>,
}

/// Parse a .jgl file content into blocks
pub fn parse_jgl(content: &str) -> Result<JglFile, JglcError> {
    let chars: Vec<char> = content.chars().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        // Skip whitespace
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }

        if pos >= chars.len() {
            break;
        }

        // Try to read a block header (look for pattern ending with {)
        let header_start = pos;
        let mut header_end = pos;
        
        // Read until we find '{' or end of line
        while header_end < chars.len() && chars[header_end] != '{' && chars[header_end] != '\n' && chars[header_end] != '\r' {
            header_end += 1;
        }

        if header_end >= chars.len() {
            // No more blocks
            break;
        }

        // If we hit a newline before finding '{', this is not a block header
        if chars[header_end] == '\n' || chars[header_end] == '\r' {
            // Skip to next line
            while pos < chars.len() && chars[pos] != '\n' && chars[pos] != '\r' {
                pos += 1;
            }
            continue;
        }

        let header_str: String = chars[header_start..header_end].iter().collect();
        let trimmed_header = header_str.trim();

        // Check if this is a valid block header
        if !trimmed_header.contains('.') {
            // Not a block header - skip this line
            pos = header_end + 1; // Skip past the '{'
            // Skip to next newline
            while pos < chars.len() && chars[pos] != '\n' && chars[pos] != '\r' {
                pos += 1;
            }
            continue;
        }

        // Position pos at the '{'
        pos = header_end;

        // Parse the block body (including nested braces)
        match parse_block_body_balanced(&chars, &mut pos) {
            Some((body_chars, body_start, body_end)) => {
                // pos now points after the closing '}'
                let body: String = body_chars.iter().collect();
                
                // Parse the header to determine block type
                let block = parse_header(trimmed_header, body);
                blocks.push(block);
            }
            None => {
                return Err(JglcError::Parse(format!(
                    "Unclosed block starting at position {}",
                    header_start
                )));
            }
        }
    }

    // Process blocks
    let mut result = JglFile::default();
    let mut unnamed_indices: HashMap<String, usize> = HashMap::new();

    for block in blocks {
        if block.is_manifest {
            if block.is_project_manifest {
                result.manifest = Some(block);
            }
            // Non-project ._jgl blocks are discarded per spec
        } else {
            // Handle unnamed blocks - assign sequential names
            let processed = if !block.is_named {
                let lang = &block.language;
                let count = unnamed_indices.entry(lang.clone()).or_insert(0);
                *count += 1;
                
                Block {
                    name: format!("v{}", count),
                    is_named: true,
                    ..block
                }
            } else {
                block
            };
            result.blocks.push(processed);
        }
    }

    Ok(result)
}

/// Parse the block header string into a Block struct (with empty body)
fn parse_header(header: &str, body: String) -> Block {
    if header.contains("._jgl") {
        // Manifest block
        let name = if header == "._jgl" {
            String::new()
        } else {
            // Has a name prefix like "project._jgl"
            header.trim_end_matches("._jgl").to_string()
        };

        return Block {
            header: header.to_string(),
            name: name.clone(),
            language: "jgl".to_string(),
            is_named: !name.is_empty(),
            body,
            is_manifest: true,
            is_project_manifest: name == "project",
        };
    }

    // Check for named blocks: name._language (e.g., "main._rs")
    if let Some(pos) = header.find("._") {
        // Named block: name._language
        let name = header[..pos].to_string();
        let lang = header[pos + 2..].to_string();
        return Block {
            header: header.to_string(),
            name,
            language: lang,
            is_named: true,
            body,
            is_manifest: false,
            is_project_manifest: false,
        };
    }

    // Must be an unnamed block: .language (e.g., ".rs")
    // These start with '.' and don't contain '._'
    if header.starts_with('.') && !header.starts_with("._") {
        let lang = header.trim_start_matches('.').to_string();
        return Block {
            header: header.to_string(),
            name: String::new(),
            language: lang,
            is_named: false,
            body,
            is_manifest: false,
            is_project_manifest: false,
        };
    }

    // Shouldn't happen with valid headers
    Block {
        header: header.to_string(),
        name: String::new(),
        language: String::new(),
        is_named: false,
        body,
        is_manifest: false,
        is_project_manifest: false,
    }
}

/// Parse a block body starting at '{' (pos points to '{'), advancing pos to after closing '}'
/// Returns the characters between the outermost braces
fn parse_block_body_balanced(chars: &[char], pos: &mut usize) -> Option<(Vec<char>, usize, usize)> {
    if *pos >= chars.len() || chars[*pos] != '{' {
        return None;
    }

    let mut depth = 0i32;
    let body_start = *pos + 1;
    let mut i = *pos;
    let mut str_state: Option<char> = None; // None, Some('"'), Some('`')


    while i < chars.len() {
        let c = chars[i];

        // Handle characters inside strings
        if let Some(quote) = str_state {
            if c == '\\' && i + 1 < chars.len() {
                // Escape sequence - skip next char
                i += 2;
                continue;
            }
            if c == quote {
                // For template literals, check for closing backtick
                // For regular strings, check for same quote type
                str_state = None;
                i += 1;
                continue;
            }
            // Inside a string, just skip the character
            i += 1;
            continue;
        }

        // Not in a string - check for block delimiters and string starts
        match c {
            '"' | '\'' | '`' => {
                str_state = Some(c);
            }
            '{' => {
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let body_chars: Vec<char> = chars[body_start..i].to_vec();
                    *pos = i + 1;
                    return Some((body_chars, body_start, i));
                }
            }
            '/' => {
                // Handle comments (but not regex literals)
                if i + 1 < chars.len() {
                    if chars[i + 1] == '/' {
                        // Line comment - skip to end of line
                        while i < chars.len() && chars[i] != '\n' {
                            i += 1;
                        }
                    } else if chars[i + 1] == '*' {
                        // Block comment - skip to */
                        i += 2;
                        while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                            i += 1;
                        }
                        if i + 1 < chars.len() {
                            i += 2;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    None
}
