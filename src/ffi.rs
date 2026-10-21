// FFI bridge generation — type-aware method signatures
//
// Method signatures are parsed out of the project's real source files, so the
// generated C stubs follow whatever the sources actually declare: any number of
// methods per file, any number of files per language, any parameter count, any
// mix of parameter types and any return type.

use regex::Regex;
use std::collections::HashMap;

/// A parsed method signature
#[derive(Debug, Clone)]
pub struct MethodSig {
    /// The method name. It is also the name of the generated C function.
    pub name: String,
    /// The return type exactly as written in the source. Empty when the source
    /// does not declare one (Python without an annotation, Lua, ...).
    pub return_type: String,
    /// The parameters, in declaration order.
    pub params: Vec<Param>,
    /// The class, object or module the method belongs to. For Java and C# this
    /// is the fully qualified name, for Python the name of the enclosing
    /// class. None when the method does not live in a class.
    pub qualifier: Option<String>,
    /// The expression that invokes the method in its own language, e.g.
    /// "Ops.area" for a Python method and "M.floor" for a Lua function that
    /// lives in a table.
    pub call_expr: String,
    /// A second expression that may also reach the method. Lua needs it: a
    /// function declared as "M.f" is published as "f", so both the path and
    /// the plain name have to be tried.
    pub alt_call_expr: Option<String>,
    /// The table a method declared with the Lua colon syntax belongs to.
    /// "function M:f(x)" is "function M.f(M, x)", so the bridge has to pass
    /// the table as the first argument.
    pub receiver_table: Option<String>,
    /// Whether the method needs an instance to be called on. A Python method
    /// that declares "self" is an instance method and cannot be called
    /// through the class alone.
    pub needs_receiver: bool,
    /// What kind of Python method this is.
    pub py: PyFlags,
    /// The method is a property: its value is read, it is not called.
    pub property: bool,
    /// Why the method cannot be called from C, when it cannot. The bridge
    /// reports the reason instead of failing in an obscure way.
    pub not_callable: Option<String>,
    /// The method needs an instance to be called on. A Java, Kotlin, Scala
    /// or C# method that is not static cannot be called from C.
    pub needs_instance: bool,
    /// The method cannot be exposed through the bridge at all. The reason
    /// is told to the user when the project is generated, and the method is
    /// left out of the header.
    pub not_exposed: Option<String>,
}

/// What kind of Python method a signature describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PyFlags {
    /// The method is a static method: it is called through the class, and
    /// it has no receiver.
    pub static_method: bool,
    /// The method is a class method: the class is its first argument.
    pub class_method: bool,
    /// The method is a coroutine: it has to be run on an event loop.
    pub coroutine: bool,
    /// The method is a generator: it yields the value it returns.
    pub generator: bool,
}

/// A parsed parameter
#[derive(Debug, Clone)]
pub struct Param {
    /// The parameter name.
    pub name: String,
    /// The type the parameter maps to.
    pub jgl_type: JglType,
    /// The type exactly as written in the source. Empty when the source does
    /// not annotate the parameter.
    pub raw_type: String,
    /// The parameter collects the rest of the arguments ("*args", "...xs").
    pub rest: bool,
    /// The parameter can only be passed by keyword: it stands behind a bare
    /// "*" or it is a "**name" catch-all.
    pub keyword_only: bool,
    /// The parameter is a catch-all of keywords ("**name"). It is handed
    /// its value by keyword, and it is part of the C signature.
    pub catch_all: bool,
    /// The parameter has a default value, so the caller may leave it out.
    pub has_default: bool,
}

/// The types a parameter or a return value can have
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JglType {
    Double,
    Int,
    String,
    Bool,
    Array,
    Object,
}

impl JglType {
    /// The C type a value of this type is passed as.
    pub fn to_c_type(&self) -> &'static str {
        match self {
            JglType::Double => "double",
            JglType::Int => "long",
            JglType::String => "*const char",
            JglType::Bool => "int",
            JglType::Array => "*const char",
            JglType::Object => "*const char",
        }
    }

    /// Map a source type name onto a JGL type.
    pub fn parse(type_str: &str) -> Self {
        let t = normalize_type_name(type_str);
        match t.as_str() {
            "double" | "float" | "number" | "real" | "decimal" | "single" | "f64" | "f32" => {
                JglType::Double
            }
            "int" | "integer" | "long" | "short" | "byte" | "i32" | "i64" | "int32" | "int64"
            | "uint" | "uint32" | "uint64" | "size_t" | "bigint" => JglType::Int,
            "string" | "str" | "text" | "char" | "charsequence" => JglType::String,
            "bool" | "boolean" => JglType::Bool,
            "array" | "list" | "seq" | "vector" | "slice" | "tuple" => JglType::Array,
            "object" | "any" | "var" | "unknown" | "void" | "none" | "unit" | "null" => {
                JglType::Object
            }
            _ => {
                // Anything array shaped is a collection, everything else is an
                // opaque object.
                if t.ends_with("[]") {
                    JglType::Array
                } else {
                    JglType::Object
                }
            }
        }
    }
}

impl Param {
    /// A parameter without a declared type: dynamically typed, so it is
    /// handled as a number unless the caller says otherwise. This is the
    /// one place where that rule is written down.
    pub fn untyped(name: &str) -> Self {
        Param {
            name: name.to_string(),
            jgl_type: JglType::Double,
            raw_type: String::new(),
            rest: false,
            keyword_only: false,
            catch_all: false,
            has_default: false,
        }
    }

    /// A parameter with a declared type.
    pub fn typed(name: &str, raw_type: &str) -> Self {
        Param {
            name: name.to_string(),
            jgl_type: JglType::parse(raw_type),
            raw_type: raw_type.to_string(),
            rest: false,
            keyword_only: false,
            catch_all: false,
            has_default: false,
        }
    }
}

/// Normalise a source type name: lower case, without generic arguments,
/// qualifiers or whitespace, so that "Map<String, Integer>" and "int" can be
/// compared.
pub fn normalize_type_name(type_str: &str) -> String {
    let mut s = type_str.trim().to_lowercase();
    // Drop the generic arguments: "map<string, integer>" -> "map"
    if let Some(p) = s.find('<') {
        s.truncate(p);
    }
    for qualifier in ["const ", "final ", "unsigned ", "signed ", "volatile "] {
        if let Some(rest) = s.strip_prefix(qualifier) {
            s = rest.to_string();
        }
    }
    s = s.replace(' ', "");
    s = s.trim_end_matches('?').to_string();
    s = s.trim_end_matches("...").to_string();
    s
}

/// The C keywords and the entry point of the generated C program. A method
/// with one of these names cannot be a C function name, so it has to be
/// renamed.
///
/// The names the C library reserves are not listed here: they are a
/// property of the target, and the build looks them up there and refuses a
/// method that would take one of them over. Renaming them here would hide
/// the clash from the user, who would then have to find the new name in
/// the header.
const C_RESERVED_NAMES: &[&str] = &[
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
    "_Alignas",
    "_Alignof",
    "_Atomic",
    "_Bool",
    "_Complex",
    "_Generic",
    "_Imaginary",
    "_Noreturn",
    "_Static_assert",
    "_Thread_local",
    // The entry point of the generated C program. A method with this name
    // would be a second definition of "main" and the link would fail.
    "main",
];

/// Turn a method name into a name a C function may have.
///
/// A method name comes from a source language, so it may be a C keyword
/// ("int", "while", ...) or start with a digit. Neither can be a C
/// function name, so the name is prefixed with "jgl_". The prefix keeps
/// the symbol recognisable and cannot collide with a name the sources
/// already use, because the sources would have to name a method "jgl_int"
/// for that.
pub fn c_function_name(name: &str) -> String {
    let is_keyword = C_RESERVED_NAMES.contains(&name);
    let starts_with_digit = name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if is_keyword || starts_with_digit {
        format!("jgl_{}", name)
    } else {
        name.to_string()
    }
}

/// Parse all method signatures from source code for a given language
pub fn parse_methods(lang: &str, source: &str) -> Vec<MethodSig> {
    parse_methods_with_pos(lang, source)
        .into_iter()
        .map(|(_, m)| m)
        .collect()
}

/// A class declaration: its offset, its name and its indentation.
#[derive(Debug, Clone)]
struct ClassDecl {
    pos: usize,
    name: String,
    indent: usize,
    /// The kind of the declaration: "class", "interface", "namespace", ...
    kind: String,
    /// The declaration is published by its module: "export class C".
    exported: bool,
    /// The type parameters the declaration names: "class Gen<T>" names one.
    type_params: Vec<String>,
    /// The classes the declaration inherits from: "class J(Base)".
    bases: Vec<String>,
}

/// Parse the methods of a source file, keeping the offset of each method so
/// that the enclosing class can be determined.
fn parse_methods_with_pos(lang: &str, source: &str) -> Vec<(usize, MethodSig)> {
    let mut methods = match lang {
        "java" => parse_java_methods(source),
        "kt" => parse_kotlin_methods(source),
        "scala" => parse_scala_methods(source),
        "cs" => parse_cs_methods(source),
        "py" => parse_python_methods(source),
        "lua" => parse_lua_methods(source),
        "ts" | "tsx" | "jsx" | "js" => parse_ts_methods(lang, source),
        _ => vec![],
    };
    // Attach the enclosing class to every method.
    let decls = class_declarations(source, lang);
    let package = package_name(source, lang);
    // The constructors of the classes of a Python source. A method of a
    // class needs an instance, and an instance needs a constructor.
    let py_constructors = if lang == "py" {
        python_constructors(source)
    } else {
        Vec::new()
    };
    // The bodies of the functions of a Python source. A class that is
    // declared inside a function is local to that function, and its
    // methods cannot be reached from outside it.
    let py_blocks = if lang == "py" {
        python_def_blocks(source)
    } else {
        Vec::new()
    };
    // The classes a TypeScript method is reached through: a function of a
    // namespace is reached through the namespace, and a static method of
    // a class through the class.
    let relevant: Vec<ClassDecl> = if matches!(lang, "ts" | "tsx" | "jsx" | "js") {
        decls
            .iter()
            .filter(|d| matches!(d.kind.as_str(), "class" | "namespace" | "module"))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    for (pos, m) in methods.iter_mut() {
        // The offset of the method name, not of the line: the line offset
        // includes the indentation, which is what has to be compared.
        let indent = line_indent(source, *pos);
        // The classes that hold the method.
        let chain: Vec<&ClassDecl> = match lang {
            "ts" | "tsx" | "jsx" | "js" => enclosing_class_decls(&relevant, *pos, indent),
            _ => enclosing_class_decls(&decls, *pos, indent),
        };
        let simple = match lang {
            // The JVM languages know a class inside a class, and the class
            // the compiler makes of it is the chain of the names.
            "java" | "kt" | "scala" => join_names(&chain, "$"),
            // Python knows a class inside a class as well, and the class is
            // reached by its dotted path.
            "py" => join_names(&chain, "."),
            // A function of a namespace is reached through the namespace,
            // and a static method of a class through the class.
            "ts" | "tsx" | "jsx" | "js" => join_names(&chain, "."),
            _ => decls
                .iter()
                .rev()
                .find(|d| d.pos < *pos)
                .map(|d| d.name.clone()),
        };
        m.qualifier = match simple {
            Some(name) => match &package {
                Some(pkg) => Some(format!("{}.{}", pkg, name)),
                None => Some(name),
            },
            None => None,
        };
        // How the method is invoked from inside its own language. A method of
        // a Python class is called through the class, a Lua function keeps
        // the path it was declared with, everything else is called by its
        // plain name.
        m.call_expr = match lang {
            "py" => match &m.qualifier {
                Some(q) => format!("{}.{}", q, m.name),
                None => m.name.clone(),
            },
            "lua" => {
                if m.call_expr.is_empty() {
                    m.name.clone()
                } else {
                    m.call_expr.clone()
                }
            }
            // A function of a namespace is reached through the namespace,
            // and a static method of a class through the class.
            "ts" | "tsx" | "jsx" | "js" => match &m.qualifier {
                Some(q) => format!("{}.{}", q, m.name),
                None => m.name.clone(),
            },
            _ => m.name.clone(),
        };
        // A method that the language itself has put out of reach cannot be
        // called from C, and it is not exposed at all.
        match lang {
            // A class that is declared inside a function is local to that
            // function: its name means nothing outside it.
            "py" => {
                if let Some(class) = chain.last() {
                    if inside_a_function(&py_blocks, class.pos) {
                        m.not_exposed = Some(format!(
                            "the class \"{}\" is declared inside a function, so the method \"{}\" cannot be reached from C",
                            class.name, m.name
                        ));
                    }
                }
            }
            // A class that the module does not publish is not part of the
            // surface of the module, and a method of it cannot be reached.
            "ts" | "tsx" => {
                if let Some(hidden) = chain.iter().find(|d| !d.exported) {
                    m.not_exposed = Some(format!(
                        "the class \"{}\" is not exported, so the method \"{}\" cannot be reached from C",
                        hidden.name, m.name
                    ));
                }
            }
            // A member of a class is an instance method, and an instance
            // cannot be made from C. A member of an object, and a function
            // of a file, are static.
            "kt" | "scala" => {
                let is_static = match chain.last() {
                    Some(class) => class.kind == "object",
                    None => true,
                };
                m.needs_instance = !is_static;
            }
            // A parameter that has the type parameter of the class cannot
            // be handed over: the class is not closed, and the value of
            // the parameter would have to be a value of that type.
            "cs" => {
                if let Some(class) = chain.last() {
                    for p in &m.params {
                        let raw = normalize_type_name(&p.raw_type);
                        let is_type_param = class
                            .type_params
                            .iter()
                            .any(|t| normalize_type_name(t) == raw);
                        if is_type_param {
                            m.not_callable = Some(format!(
                                "the parameter \"{}\" has the type parameter \"{}\" of the class \"{}\", so the method \"{}\" cannot be called from C",
                                p.name, raw, class.name, m.name
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
        // A method of a Python class needs an instance, and the instance
        // needs a constructor that can be called without arguments.
        if lang == "py" && m.needs_receiver {
            if let Some(q) = m.qualifier.clone() {
                if let Some((_, needed)) = py_constructors.iter().find(|(c, _)| *c == q) {
                    if *needed > 0 {
                        m.not_callable = Some(format!(
                            "the constructor of the class \"{}\" needs {} argument(s), so the method \"{}\" cannot be called from C",
                            q, needed, m.name
                        ));
                    }
                }
            }
        }
    }
    methods
}

/// The chain of the names of a class chain, joined the way the language
/// joins them.
fn join_names(chain: &[&ClassDecl], joiner: &str) -> Option<String> {
    if chain.is_empty() {
        None
    } else {
        Some(
            chain
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(joiner),
        )
    }
}

/// The bodies of the functions of a Python source: the offset of the name
/// of the function, the indentation of its definition and the offset of
/// the end of its body.
fn python_def_blocks(source: &str) -> Vec<(usize, usize, usize)> {
    let re = Regex::new(r"(?m)^[ \t]*(?:async[ \t]+)?def[ \t]+(\w+)").unwrap();
    let mut out = Vec::new();
    for caps in re.captures_iter(source) {
        let name_pos = caps.get(1).unwrap().start();
        let indent = line_indent(source, name_pos);
        // The body of the function ends at the first line that is not
        // deeper than the definition itself.
        let mut offset = caps.get(0).unwrap().end();
        let rest = &source[offset..];
        offset += rest.find('\n').map(|i| i + 1).unwrap_or(rest.len());
        let mut end = source.len();
        for line in source[offset..].lines() {
            let start = offset;
            offset += line.len() + 1;
            if line.trim().is_empty() {
                continue;
            }
            let depth = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if depth <= indent {
                end = start;
                break;
            }
        }
        out.push((name_pos, indent, end));
    }
    out
}

/// Whether the position is inside the body of a function.
fn inside_a_function(blocks: &[(usize, usize, usize)], pos: usize) -> bool {
    blocks
        .iter()
        .any(|(start, _, end)| *start < pos && pos < *end)
}

/// The indentation of the line the given offset is on.
fn line_indent(source: &str, pos: usize) -> usize {
    let line_start = source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    source[line_start..pos]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count()
}

/// The chain of the classes that enclose the method at `pos`.
///
/// A class may live in another class, and the class the compiler makes of
/// it then carries the chain of the names. The JVM languages join the names
/// with a dollar sign, Python with a dot. Only the classes that enclose the
/// method count, so that a file level function is not read as a method of
/// the class above it.
fn enclosing_class_chain(
    decls: &[ClassDecl],
    pos: usize,
    indent: usize,
    joiner: &str,
) -> Option<String> {
    let chain = enclosing_class_decls(decls, pos, indent);
    if chain.is_empty() {
        None
    } else {
        Some(
            chain
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
                .join(joiner),
        )
    }
}

/// The classes that enclose the method at `pos`.
///
/// A class may live in another class, and the class the compiler makes of
/// it then carries the chain of the names. The JVM languages join the names
/// with a dollar sign, Python with a dot. Only the classes that enclose the
/// method count, so that a file level function is not read as a method of
/// the class above it.
fn enclosing_class_decls<'a>(
    decls: &'a [ClassDecl],
    pos: usize,
    indent: usize,
) -> Vec<&'a ClassDecl> {
    let mut chain: Vec<&ClassDecl> = Vec::new();
    let mut chain_indent = 0usize;
    for d in decls.iter().filter(|d| d.pos < pos && d.indent < indent) {
        // A class that stands no deeper than the one before it starts a
        // new branch and closes the one before.
        if !chain.is_empty() && d.indent <= chain_indent {
            chain.clear();
        }
        chain.push(d);
        chain_indent = d.indent;
    }
    chain
}

/// The name the class itself is reached by: the chain of the names of the
/// classes that hold it, and its own name.
fn class_qualifier(decls: &[ClassDecl], class: &ClassDecl, joiner: &str) -> String {
    match enclosing_class_chain(decls, class.pos, class.indent, joiner) {
        Some(parents) => format!("{}{}{}", parents, joiner, class.name),
        None => class.name.clone(),
    }
}

/// The keywords that introduce a class-like declaration, per language.
///
/// The set is per language because the same word means different things:
/// "object" introduces a singleton in Kotlin and Scala, but in C# it is
/// the type every other type derives from, so a method that returns
/// "object" must not be read as the declaration of a class.
fn declaration_keywords(lang: &str) -> &'static str {
    match lang {
        "java" => "class|interface|enum|record",
        "kt" => "class|object|interface|enum",
        "scala" => "class|object|trait|enum",
        "cs" => "class|record|struct|interface|enum",
        "ts" | "tsx" | "jsx" | "js" => "class|interface|enum|namespace|module",
        _ => "class|interface|enum|record|struct",
    }
}

/// The classes, interfaces, records and objects declared in a source file.
fn class_declarations(source: &str, lang: &str) -> Vec<ClassDecl> {
    let pattern = match lang {
        "py" => {
            r"(?m)^([ \t]*)(class)\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:\(([^)]*)\))?\s*:".to_string()
        }
        _ => format!(
            r"(?m)^([ \t]*)((?:(?:public|internal|private|protected|final|abstract|sealed|static|partial|open|data|readonly|const|virtual|override|new|export|default|case)\s+)*)(?:({})\s+)([A-Za-z_][A-Za-z0-9_]*)(?:\s*<([^>]*)>)?",
            declaration_keywords(lang)
        ),
    };
    let re = Regex::new(&pattern).unwrap();
    re.captures_iter(source)
        .map(|c| {
            // The two patterns lay their groups out differently, so the
            // fields are read according to the language.
            let (kind, name, modifiers, type_params, bases) = if lang == "py" {
                (
                    c.get(2).unwrap().as_str().to_string(),
                    c.get(3).unwrap().as_str().to_string(),
                    String::new(),
                    None,
                    c.get(4).map(|m| m.as_str().to_string()),
                )
            } else {
                (
                    c.get(3).unwrap().as_str().to_string(),
                    c.get(4).unwrap().as_str().to_string(),
                    c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
                    c.get(5).map(|m| m.as_str().to_string()),
                    None,
                )
            };
            ClassDecl {
                pos: c.get(0).unwrap().start(),
                name,
                indent: c.get(1).unwrap().as_str().chars().count(),
                kind,
                exported: modifiers.split_whitespace().any(|w| w == "export"),
                type_params: split_names(type_params.as_deref()),
                bases: split_names(bases.as_deref()),
            }
        })
        .collect()
}

/// The names a list of declarations carries: "T, U" names two, and a list
/// of base classes names the classes a class inherits from.
fn split_names(text: Option<&str>) -> Vec<String> {
    text.map(|t| {
        t.split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// The package (Java, Kotlin, Scala) or namespace (C#) a source file declares.
pub fn package_name(source: &str, lang: &str) -> Option<String> {
    let re = match lang {
        "java" | "kt" | "scala" => Regex::new(r"(?m)^\s*package\s+([\w.]+)").unwrap(),
        "cs" => Regex::new(r"(?m)^\s*namespace\s+([\w.]+)").unwrap(),
        _ => return None,
    };
    re.captures(source)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Parse Java method signatures: `<modifiers> <rtype> <name>(<params>)`
///
/// A method that is not static cannot be called from C. It is parsed
/// anyway, so that the bridge can say so instead of the linker
/// complaining about a symbol that was never there.
fn parse_java_methods(source: &str) -> Vec<(usize, MethodSig)> {
    let re = Regex::new(
        r"(?m)\b((?:(?:public|protected|private|final|abstract|static|synchronized|native|strictfp|default)\s+)+)(?:<([^>]*)>\s*)?([\w.$<>\[\]?,]+(?:[ \t]+[\w.$<>\[\]?,]+)*?)\s+(\w+)\s*\(([^)]*)",
    )
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let modifiers = caps.get(1).unwrap().as_str();
        let type_params = split_names(caps.get(2).map(|m| m.as_str()));
        let return_type = caps.get(3).unwrap().as_str().to_string();
        let name = caps.get(4).unwrap().as_str().to_string();
        let params = parse_typed_params(caps.get(5).unwrap().as_str());
        // A method that is not static needs an instance, and an instance
        // cannot be made from C.
        let needs_instance = !modifiers.split_whitespace().any(|w| w == "static");
        // A generic method erases its type parameters, so the value it
        // returns is an object when it returns one of them. An object has
        // no C representation.
        let not_callable = if type_params
            .iter()
            .any(|t| normalize_type_name(t) == normalize_type_name(&return_type))
        {
            Some(format!(
                "the method \"{}\" returns the type parameter \"{}\", whose value cannot be handed to C",
                name,
                return_type.trim()
            ))
        } else {
            None
        };
        methods.push((
            caps.get(4).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable,
                needs_instance,
                not_exposed: None,
            },
        ));
    }
    methods
}

/// Parse Kotlin method signatures: `fun <name>(<params>): <rtype>`
///
/// An extension function names the type it extends before its own name.
/// The receiver is the first argument of the method the compiler makes of
/// it, so it counts as a parameter.
fn parse_kotlin_methods(source: &str) -> Vec<(usize, MethodSig)> {
    let re = Regex::new(
        r"(?m)\bfun\s+(?:<[^>]*>\s*)?(?:([\w.]+)\.)?(\w+)\s*\(([^)]*)\)\s*(?::\s*([\w.]+(?:\s*<[^<>]*>)?(?:\[\])*\??))?",
    )
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let receiver = caps.get(1).map(|m| m.as_str().to_string());
        let name = caps.get(2).unwrap().as_str().to_string();
        let mut params = parse_colon_params(caps.get(3).unwrap().as_str());
        // The receiver of an extension function is the first argument of
        // the method. It is a value of the type the function extends.
        if let Some(receiver) = receiver {
            let raw_type = receiver.to_lowercase();
            let mut param = Param::typed("this", &raw_type);
            param.rest = false;
            params.insert(0, param);
        }
        let return_type = caps
            .get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        methods.push((
            caps.get(2).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance: false,
                not_exposed: None,
            },
        ));
    }
    methods
}

/// Parse Scala method signatures: `def <name>(<params>): <rtype>`
fn parse_scala_methods(source: &str) -> Vec<(usize, MethodSig)> {
    let re = Regex::new(
        r"(?m)\bdef\s+(\w+)\s*\(([^)]*)\)\s*(?::\s*([\w.]+(?:\s*<[^<>]*>)?(?:\[\])*\??))?",
    )
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let name = caps.get(1).unwrap().as_str().to_string();
        let params = parse_colon_params(caps.get(2).unwrap().as_str());
        let return_type = caps
            .get(3)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        methods.push((
            caps.get(1).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance: false,
                not_exposed: None,
            },
        ));
    }
    methods
}

/// Parse C# method signatures: `<modifiers> <rtype> <name>(<params>)`
///
/// A method that is not static cannot be called from C. It is parsed
/// anyway, so that the bridge can say so instead of the linker
/// complaining about a symbol that was never there. A static property is
/// parsed as well: its getter is the method that is called.
fn parse_cs_methods(source: &str) -> Vec<(usize, MethodSig)> {
    let re = Regex::new(
        r"(?m)\b((?:(?:public|internal|private|protected|static|sealed|override|virtual|partial|new|async|extern|unsafe|readonly|const|abstract)\s+)+)([\w.$<>\[\]?,]+(?:[ \t]+[\w.$<>\[\]?,]+)*?)\s+(\w+)\s*(?:<([^>]*)>\s*)?\(([^)]*)",
    )
    .unwrap();
    // A static property: "public static double Prop { get { ... } }".
    let prop_re = Regex::new(
        r"(?m)\b((?:(?:public|internal|private|protected|static|sealed|override|virtual|partial|new|async|extern|unsafe|readonly|const|abstract)\s+)+)([\w.$<>\[\]?,]+)\s+(\w+)\s*\{",
    )
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let modifiers = caps.get(1).unwrap().as_str();
        let return_type = caps.get(2).unwrap().as_str().to_string();
        let name = caps.get(3).unwrap().as_str().to_string();
        let type_params = split_names(caps.get(4).map(|m| m.as_str()));
        let params = parse_typed_params(caps.get(5).unwrap().as_str());
        // A method that is not static needs an instance, and an instance
        // cannot be made from C.
        let needs_instance = !modifiers.split_whitespace().any(|w| w == "static");
        // A generic method cannot be called late: the type parameters it
        // declares are not bound to anything.
        let not_exposed = if type_params.is_empty() {
            None
        } else {
            Some(format!(
                "the method \"{}\" is generic, so it cannot be called from C",
                name
            ))
        };
        methods.push((
            caps.get(3).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance,
                not_exposed,
            },
        ));
    }
    for caps in prop_re.captures_iter(source) {
        // Only a static property can be read without an instance.
        let modifiers = caps.get(1).unwrap().as_str();
        if !modifiers.split_whitespace().any(|w| w == "static") {
            continue;
        }
        let return_type = caps.get(2).unwrap().as_str().trim().to_string();
        // The declaration of a class is not a property.
        if matches!(
            return_type.to_lowercase().as_str(),
            "class" | "struct" | "record" | "interface" | "enum"
        ) {
            continue;
        }
        let name = caps.get(3).unwrap().as_str().to_string();
        methods.push((
            caps.get(3).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params: Vec::new(),
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: true,
                not_callable: None,
                needs_instance: false,
                not_exposed: None,
            },
        ));
    }
    methods.sort_by_key(|(pos, _)| *pos);
    methods
}

/// Parse Python method signatures: `def <name>(<params>) -> <rtype>`
fn parse_python_methods(source: &str) -> Vec<(usize, MethodSig)> {
    let re = Regex::new(
        r"(?m)^[ \t]*(async[ \t]+)?def[ \t]+(\w+)[ \t]*\(([^)]*)\)[ \t]*(?:->[ \t]*([\w\[\]\., |]+))?[ \t]*:",
    )
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let is_async = caps.get(1).is_some();
        let name = caps.get(2).unwrap().as_str().to_string();
        // A dunder is the machinery of the language itself: a constructor, a
        // comparison, a context manager. It is not a method of the project.
        if is_dunder(&name) {
            continue;
        }
        // Annotated parameters ("name: str") carry a type, plain ones do not.
        let all_params = parse_colon_params(caps.get(3).unwrap().as_str());
        // A method inside a class has an implicit receiver as its first
        // parameter: "self" for an instance method, "cls" for a class method.
        // A static method has none, and its first parameter is a real
        // argument, so the decorator has to be looked at.
        let decorator = preceding_decorator(source, caps.get(2).unwrap().start());
        let is_static = decorator.as_deref() == Some("staticmethod");
        let is_class = decorator.as_deref() == Some("classmethod");
        let is_property = decorator.as_deref() == Some("property");
        let receiver = if is_static {
            None
        } else {
            all_params
                .first()
                .filter(|p| p.name == "self" || p.name == "cls")
                .map(|p| p.name.clone())
        };
        // A class method is called through the class, and the class is its
        // first argument: it needs no instance, and so no constructor.
        let needs_receiver = receiver.is_some() && !is_class;
        let params: Vec<Param> = match &receiver {
            Some(recv) => all_params
                .into_iter()
                .skip_while(|p| &p.name != recv)
                .skip(1)
                .collect(),
            None => all_params,
        };
        // A parameter behind a bare "*" can only be passed by keyword. The
        // bridge has no way of asking the caller for a value by name, so
        // such a parameter keeps the default value of its declaration and
        // is not part of the C signature. A parameter without a default
        // cannot be filled that way, so the method cannot be called at all.
        let mut not_callable = None;
        let mut kept: Vec<Param> = Vec::new();
        for p in params {
            if p.keyword_only && !p.catch_all {
                if !p.has_default {
                    not_callable = Some(format!(
                        "the parameter \"{}\" can only be passed by keyword and has no default value, so the method \"{}\" cannot be called from C",
                        p.name, name
                    ));
                }
                continue;
            }
            kept.push(p);
        }
        let return_type = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        // A function that yields is a generator: it hands its value out one
        // by one instead of returning it.
        let generator = python_body_yields(source, caps.get(2).unwrap().start());
        methods.push((
            caps.get(2).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params: kept,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver,
                receiver_table: None,
                py: PyFlags {
                    static_method: is_static,
                    class_method: is_class,
                    coroutine: is_async,
                    generator,
                },
                property: is_property,
                not_callable,
                needs_instance: false,
                not_exposed: None,
            },
        ));
    }
    methods
}

/// Whether the name is a dunder: a name the language reserves for its own
/// machinery, such as "__init__" or "__len__".
fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// Whether the body of the definition at `pos` yields. A function that
/// yields is a generator: it hands its value out one by one.
fn python_body_yields(source: &str, pos: usize) -> bool {
    let indent = line_indent(source, pos);
    for line in source[pos..].lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if line_indent <= indent {
            // The block of the definition ends here.
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("yield") {
            if !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                return true;
            }
        }
    }
    false
}

/// The constructors of the classes a Python source declares: the chain of
/// the names of the class and the number of arguments its constructor
/// needs, the receiver aside.
///
/// A class that declares no constructor of its own uses the one of the
/// class it inherits from, so the arguments of that one are the arguments
/// the class needs as well.
fn python_constructors(source: &str) -> Vec<(String, usize)> {
    let decls = class_declarations(source, "py");
    let re =
        Regex::new(r"(?m)^[ \t]*(?:async[ \t]+)?def[ \t]+(__init__)[ \t]*\(([^)]*)\)").unwrap();
    // The constructor every class declares itself.
    let mut own: HashMap<String, usize> = HashMap::new();
    for caps in re.captures_iter(source) {
        // The offset of the name, not of the line: the line offset includes
        // the indentation, which is what has to be compared.
        let pos = caps.get(1).unwrap().start();
        let indent = line_indent(source, pos);
        let chain = enclosing_class_chain(&decls, pos, indent, ".").unwrap_or_default();
        let params = parse_colon_params(caps.get(2).unwrap().as_str());
        // The receiver is not an argument, a parameter with a default value
        // may be left out, and a parameter that collects the rest of the
        // arguments may be empty. A parameter that can only be passed by
        // keyword and has no default value has to be given, so it counts.
        let needed = params
            .iter()
            .filter(|p| {
                p.name != "self" && p.name != "cls" && !p.rest && !p.catch_all && !p.has_default
            })
            .count();
        own.insert(chain, needed);
    }
    // The constructor of every class: the one it declares, or the one it
    // inherits.
    let mut out = Vec::new();
    for class in &decls {
        let chain = class_qualifier(&decls, class, ".");
        let needed = inherited_constructor_args(&decls, &own, class, &mut Vec::new());
        out.push((chain, needed));
    }
    out
}

/// The number of arguments the constructor of the class needs.
///
/// A class that declares no constructor of its own uses the one of the
/// class it inherits from.
fn inherited_constructor_args(
    decls: &[ClassDecl],
    own: &HashMap<String, usize>,
    class: &ClassDecl,
    seen: &mut Vec<String>,
) -> usize {
    let chain = class_qualifier(decls, class, ".");
    // A class that inherits from itself would never stop.
    if seen.contains(&chain) {
        return 0;
    }
    seen.push(chain.clone());
    if let Some(&needed) = own.get(&chain) {
        return needed;
    }
    for base in &class.bases {
        // The base class is looked up by its plain name: a class of another
        // file is not known here.
        if let Some(base_class) = decls.iter().rev().find(|d| &d.name == base) {
            return inherited_constructor_args(decls, own, base_class, seen);
        }
    }
    0
}

/// The decorator that decorates the definition starting at `pos`, if any.
/// Python decorators sit on the line above the definition they decorate.
fn preceding_decorator(source: &str, pos: usize) -> Option<String> {
    let line_start = source[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before = &source[..line_start];
    // The line above the definition: the text between the last two line
    // breaks. The text before the definition ends with a line break of its
    // own, so the last one does not count.
    let prev_end = before.trim_end_matches('\n').len();
    let prev_line = match before[..prev_end].rfind('\n') {
        Some(i) => &before[i + 1..prev_end],
        None => &before[..prev_end],
    };
    let trimmed = prev_line.trim();
    let rest = trimmed.strip_prefix('@')?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}
/// Parse Lua method signatures.
///
/// A Lua function is declared as `function name(args)`, as
/// `function Table.name(args)` or as `Table.name = function(args)`. Lua is
/// dynamically typed, so the types of the parameters and of the return value
/// are only known when the source annotates them with the LuaCATS comments
/// `---@param <name> <type>` and `---@return <type>`.
fn parse_lua_methods(source: &str) -> Vec<(usize, MethodSig)> {
    // A function is declared as "function name(...)", as "function Table.name(...)"
    // or as "Table.name = function(...)". Lua is dynamically typed, so the
    // types of the parameters and of the return value are only known when
    // the source annotates them with the LuaCATS comments.
    const PATH: &str = r"[A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*";
    let re = Regex::new(&format!(r"(?m)\bfunction\s+({})\s*\(([^)]*)\)", PATH)).unwrap();
    // A method declared with the colon syntax, "function Table:name(...)", is
    // "function Table.name(Table, ...)", so the table is passed as the first
    // argument. It gets its own pattern, so that the capture groups of the
    // other forms keep their meaning.
    let colon = Regex::new(&format!(
        r"(?m)\bfunction\s+({}):([A-Za-z_]\w*)\s*\(([^)]*)\)",
        PATH
    ))
    .unwrap();
    let assign = Regex::new(&format!(r"(?m)\b({})\s*=\s*function\s*\(([^)]*)\)", PATH)).unwrap();
    // The table a module returns names the functions it exports: "return {
    // circle_area = area }" exports the local function "area" under the name
    // "circle_area".
    let exports = lua_module_exports(source);
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let start = caps.get(0).unwrap().start();
        let path = caps.get(1).unwrap().as_str();
        // A function that is local to the file cannot be reached from
        // outside it, unless the module publishes it.
        if is_local_definition(source, start) && !lua_is_exported(&exports, path) {
            continue;
        }
        methods.push(lua_method(
            source,
            start,
            path,
            caps.get(2).unwrap().as_str(),
            None,
            &exports,
        ));
    }
    for caps in colon.captures_iter(source) {
        // The table is the path of the declaration, the method is the name
        // after the colon.
        let table = caps.get(1).unwrap().as_str();
        let name = caps.get(2).unwrap().as_str();
        let full = format!("{}.{}", table, name);
        methods.push(lua_method(
            source,
            caps.get(0).unwrap().start(),
            &full,
            caps.get(3).unwrap().as_str(),
            Some(table),
            &exports,
        ));
    }
    for caps in assign.captures_iter(source) {
        let start = caps.get(0).unwrap().start();
        let path = caps.get(1).unwrap().as_str();
        // Skip the ones the first pattern already found. The path is what
        // tells two declarations apart, not the plain name.
        if methods.iter().any(|(_, m)| lua_path(m) == path) {
            continue;
        }
        // A function that is bound to a local variable cannot be reached
        // from outside the file, unless the module publishes it.
        if is_local_definition(source, start) && !lua_is_exported(&exports, path) {
            continue;
        }
        methods.push(lua_method(
            source,
            start,
            path,
            caps.get(2).unwrap().as_str(),
            None,
            &exports,
        ));
    }
    methods.sort_by_key(|(pos, _)| *pos);
    methods
}

/// Whether the definition at `pos` is preceded by the word "local". A local
/// definition is not reachable from outside the file it lives in.
fn is_local_definition(source: &str, pos: usize) -> bool {
    let before = source[..pos].trim_end();
    match before
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
    {
        Some(word) => word == "local",
        None => false,
    }
}

/// Whether the module publishes the function with the given path.
fn lua_is_exported(exports: &[(String, String)], path: &str) -> bool {
    let plain = path.rsplit('.').next().unwrap_or(path);
    exports.iter().any(|(local, _)| local == plain)
}

/// The path a Lua function was declared with.
fn lua_path(m: &MethodSig) -> &str {
    m.alt_call_expr.as_deref().unwrap_or(&m.name)
}

/// The functions of a Lua source that cannot be called from C: they are
/// local to the file and the module does not publish them.
pub fn lua_local_functions(source: &str) -> Vec<String> {
    let exports = lua_module_exports(source);
    let re = Regex::new(r"(?m)\blocal\s+(?:function\s+)?([A-Za-z_]\w*)\s*(?:\(|=\s*function\s*\()")
        .unwrap();
    let mut out = Vec::new();
    for caps in re.captures_iter(source) {
        let name = caps.get(1).unwrap().as_str();
        if !lua_is_exported(&exports, name) {
            out.push(name.to_string());
        }
    }
    out
}

/// The functions a Lua module exports: the local name and the name it is
/// exported under.
fn lua_module_exports(source: &str) -> Vec<(String, String)> {
    let re = Regex::new(r"(?m)\breturn\s*\{([^}]*)\}").unwrap();
    let mut out = Vec::new();
    for caps in re.captures_iter(source) {
        for entry in caps.get(1).unwrap().as_str().split(',') {
            let entry = entry.trim();
            let Some((key, value)) = entry.split_once('=') else {
                continue;
            };
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            if key.is_empty() || value.is_empty() {
                continue;
            }
            out.push((value, key));
        }
    }
    out
}

/// Build a method signature from a matched Lua function definition.
///
/// `path` is the name the function is declared with, `params` its parameter
/// list, and `receiver_table` the table of a method declared with the colon
/// syntax, which is passed as the first argument.
fn lua_method(
    source: &str,
    pos: usize,
    path: &str,
    params: &str,
    receiver_table: Option<&str>,
    exports: &[(String, String)],
) -> (usize, MethodSig) {
    let path = path.to_string();
    // The C symbol is the name the function is exported under, so that it
    // matches the name the module publishes it as.
    let plain = path.rsplit('.').next().unwrap_or(&path).to_string();
    let name = exports
        .iter()
        .find(|(local, _)| *local == plain)
        .map(|(_, exported)| exported.clone())
        .unwrap_or_else(|| plain.clone());
    // A function declared as "M.f" is published as "f", but it is also
    // reachable through the table it was declared in. Both paths are kept so
    // that the bridge can try either one.
    let alt_call_expr = if path == name {
        None
    } else {
        Some(path.clone())
    };
    let mut params = parse_untyped_params(params);
    let (ann_params, ann_return) = lua_annotations(source, pos);
    // Annotations name their parameter, but they may also be given in order
    // when the name is omitted.
    let param_count = params.len();
    for (idx, p) in params.iter_mut().enumerate() {
        let annotated = ann_params.iter().find(|(n, _)| *n == p.name).or_else(|| {
            if ann_params.len() == param_count {
                ann_params.get(idx)
            } else {
                None
            }
        });
        if let Some((_, t)) = annotated {
            let raw = t.clone();
            p.jgl_type = JglType::parse(&raw);
            p.raw_type = raw;
        }
    }
    (
        pos,
        MethodSig {
            // The call goes through the exported name, because that is how
            // the module publishes the function.
            call_expr: name.clone(),
            alt_call_expr,
            name,
            return_type: ann_return.unwrap_or_default(),
            params,
            qualifier: None,
            receiver_table: receiver_table.map(|t| t.to_string()),
            needs_receiver: false,
            py: PyFlags::default(),
            property: false,
            not_callable: None,
            needs_instance: false,
            not_exposed: None,
        },
    )
}

/// The LuaCATS annotations (`---@param`, `---@return`) that precede a
/// function definition.
fn lua_annotations(source: &str, def_pos: usize) -> (Vec<(String, String)>, Option<String>) {
    let mut params = Vec::new();
    let mut ret = None;
    let head = &source[..def_pos];
    for line in head.lines().rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("---@param") {
            let mut it = rest.split_whitespace();
            let name = it.next().unwrap_or("").to_string();
            let ty = it.next().unwrap_or("").to_string();
            if !name.is_empty() && !ty.is_empty() {
                params.push((name, ty));
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("---@return") {
            if let Some(ty) = rest.split_whitespace().next() {
                ret = Some(ty.to_string());
            }
            continue;
        }
        // Only the block of annotation lines directly above the function counts.
        if !trimmed.is_empty() {
            break;
        }
    }
    params.reverse();
    (params, ret)
}

/// Parse TypeScript/JavaScript method signatures.
///
/// A function is declared as "function <name>(...)", as
/// "const <name> = function (...)", as "const <name> = (...) => ..." or as
/// a static method of a class, "static <name>(...)".
fn parse_ts_methods(lang: &str, source: &str) -> Vec<(usize, MethodSig)> {
    // A type is a name, an optional list of generic arguments, an optional
    // array suffix or a tuple. A union of those is a type as well.
    const TYPE: &str = r"(?:[\w.]+(?:\s*<[^<>]*>)?|\[[^\]]*\])(?:\[\])*(?:\s*\|\s*(?:[\w.]+(?:\s*<[^<>]*>)?|\[[^\]]*\])(?:\[\])*)*";
    let re = Regex::new(&format!(
        r"(?m)\b(?:(export\s+default|export)\s+)?(?:async\s+)?function\s+(\w+)\s*(?:<[^>]*>\s*)?\(([^)]*)\)\s*(?::\s*({}))?\s*(?:\{{|=>)",
        TYPE
    ))
    .unwrap();
    // Only TypeScript publishes its functions with the "export" keyword.
    // JavaScript publishes them with "module.exports", and a function of a
    // file may be published that way.
    let published = uses_export_keyword(lang);
    // A default export is published under the name "default", whatever the
    // function itself is called, so the export has to be recognised.
    let default_re = Regex::new(r"(?m)\bexport\s+default\s+(?:async\s+)?function\s+(\w+)").unwrap();
    // A function expression or an arrow function bound to a constant. The
    // constant may be a "const", a "let" or a "var", and the arrow function
    // may have a list of parameters or a single one without a list.
    let bound = Regex::new(&format!(
        r"(?m)\b(?:(export)\s+)?(?:const|let|var)\s+(\w+)\s*(?::\s*({}))?\s*=\s*(?:async\s*)?(?:function\s*(?:\*\s*)?\(([^)]*)\)|\(([^)]*)\)|(\w+))\s*(?::\s*({}))?\s*(?:\{{|=>)",
        TYPE, TYPE
    ))
    .unwrap();
    // A static method of a class: it needs no instance to be called.
    let statics = Regex::new(&format!(
        r"(?m)^[ \t]*static\s+(\w+)\s*(?:<[^>]*>\s*)?\(([^)]*)\)\s*(?::\s*({}))?\s*\{{",
        TYPE
    ))
    .unwrap();
    let mut methods = Vec::new();
    for caps in re.captures_iter(source) {
        let exported = caps.get(1).is_some();
        let name = caps.get(2).unwrap().as_str().to_string();
        let params = parse_colon_params(caps.get(3).unwrap().as_str());
        let return_type = caps
            .get(4)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        // A default export is published under the name "default", so the
        // bridge has to look it up under that name.
        let alt_call_expr = if is_default_export(&default_re, &name, source) {
            Some("default".to_string())
        } else {
            None
        };
        // A function that the module does not publish is not part of the
        // surface of the module, and it cannot be reached from C.
        let not_exposed = if exported || !published {
            None
        } else {
            Some(format!(
                "the function \"{}\" is not exported, so it cannot be reached from C",
                name
            ))
        };
        methods.push((
            caps.get(2).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance: false,
                not_exposed,
            },
        ));
    }
    for caps in bound.captures_iter(source) {
        let exported = caps.get(1).is_some();
        let name = caps.get(2).unwrap().as_str().to_string();
        // The parameters of a function expression, of an arrow function with
        // a list of parameters, or the single parameter of an arrow function
        // that has no list.
        let params = match (caps.get(4), caps.get(5), caps.get(6)) {
            (Some(p), _, _) => parse_colon_params(p.as_str()),
            (None, Some(p), _) => parse_colon_params(p.as_str()),
            (None, None, Some(p)) => vec![Param::untyped(p.as_str())],
            _ => Vec::new(),
        };
        let return_type = caps
            .get(7)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        // A constant that the module does not publish is not part of the
        // surface of the module, and it cannot be reached from C.
        let not_exposed = if exported || !published {
            None
        } else {
            Some(format!(
                "the function \"{}\" is not exported, so it cannot be reached from C",
                name
            ))
        };
        methods.push((
            caps.get(2).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance: false,
                not_exposed,
            },
        ));
    }
    for caps in statics.captures_iter(source) {
        let name = caps.get(1).unwrap().as_str().to_string();
        let params = parse_colon_params(caps.get(2).unwrap().as_str());
        let return_type = caps
            .get(3)
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        methods.push((
            caps.get(1).unwrap().start(),
            MethodSig {
                name,
                return_type,
                params,
                qualifier: None,
                call_expr: String::new(),
                alt_call_expr: None,
                needs_receiver: false,
                receiver_table: None,
                py: PyFlags::default(),
                property: false,
                not_callable: None,
                needs_instance: false,
                not_exposed: None,
            },
        ));
    }
    methods
}

/// Whether the function with the given name is exported as the default
/// export of its module. A default export is published under the name
/// "default", so the bridge has to look it up under that name.
fn is_default_export(default_re: &Regex, name: &str, source: &str) -> bool {
    default_re
        .captures_iter(source)
        .any(|c| c.get(1).map(|m| m.as_str() == name).unwrap_or(false))
}

/// Whether the language publishes the functions of a module with the
/// "export" keyword.
///
/// TypeScript does, and a function that is not published is not part of the
/// surface of the module. JavaScript publishes its functions with
/// "module.exports", and any function of a file may be published that way.
fn uses_export_keyword(lang: &str) -> bool {
    matches!(lang, "ts" | "tsx")
}

/// Split a parameter list on the commas that are not nested in <>, [] or ().
fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '[' | '(' => {
                depth += 1;
                cur.push(ch);
            }
            '>' | ']' | ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// The words a parameter may carry in front of its type. They say how the
/// parameter is passed, not what it holds.
const PARAM_MODIFIERS: &[&str] = &[
    "final", "const", "ref", "out", "in", "params", "readonly", "this", "volatile",
];

/// Parse a "type name" parameter list: "double a, final int b" →
/// [Param("a", Double), Param("b", Int)]. The type is everything in front of
/// the name, so that a type with a space in it, such as
/// "Map<String, Integer>", survives whole.
fn parse_typed_params(params_str: &str) -> Vec<Param> {
    split_top_level(params_str)
        .iter()
        .filter_map(|s| {
            // A default value belongs to the declaration, not to the type.
            let (s, has_default) = match s.find('=') {
                Some(i) => (s[..i].trim(), true),
                None => (s.trim(), false),
            };
            let tokens: Vec<&str> = s.split_whitespace().collect();
            if tokens.len() < 2 {
                return None;
            }
            let name_token = tokens[tokens.len() - 1];
            let mut raw_type = tokens[..tokens.len() - 1].join(" ");
            // The modifiers in front of the type are not part of it.
            loop {
                let mut stripped = false;
                for m in PARAM_MODIFIERS {
                    if let Some(rest) = raw_type.strip_prefix(&format!("{} ", m)) {
                        raw_type = rest.trim_start().to_string();
                        stripped = true;
                        break;
                    }
                }
                if !stripped {
                    break;
                }
            }
            let mut name = name_token.to_string();
            // "double a[]" declares an array of doubles.
            if name.ends_with("[]") {
                name.truncate(name.len() - 2);
                raw_type.push_str("[]");
            }
            // "String... args" declares an array of strings.
            if let Some(base) = raw_type.strip_suffix("...") {
                raw_type = format!("{}[]", base.trim());
            }
            let mut param = Param::typed(&name, &raw_type);
            param.has_default = has_default;
            Some(param)
        })
        .collect()
}

/// Parse a "name: type" parameter list: "a: double, b: int = 3" →
/// [Param("a", Double), Param("b", Int)]. A parameter without an annotation
/// keeps its place and is treated as a number.
///
/// The list may carry the markers of Python and TypeScript as well: a bare
/// "*" starts the parameters that can only be passed by keyword, a "*name"
/// and a "...name" collect the rest of the arguments, and a "**name"
/// collects the keywords.
fn parse_colon_params(params_str: &str) -> Vec<Param> {
    let mut keyword_only = false;
    let mut out: Vec<Param> = Vec::new();
    for entry in split_top_level(params_str) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        // A default value belongs to the declaration, not to the name or
        // the type of the parameter.
        let (decl, mut has_default) = match entry.find('=') {
            Some(i) => (entry[..i].trim(), true),
            None => (entry, false),
        };
        // A bare "*" is a marker and not a parameter: the parameters behind
        // it can only be passed by keyword.
        if decl == "*" {
            keyword_only = true;
            continue;
        }
        // The markers of a parameter that collects arguments.
        let (decl, rest, catch_all) = if let Some(rest) = decl.strip_prefix("**") {
            (rest, false, true)
        } else if let Some(rest) = decl.strip_prefix("...") {
            (rest, true, false)
        } else if let Some(rest) = decl.strip_prefix('*') {
            (rest, true, false)
        } else {
            (decl, false, false)
        };
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let (mut name, raw_type) = match decl.find(':') {
            Some(pos) => {
                let head = decl[..pos].trim();
                // A name that ends with a question mark is optional.
                if head.ends_with('?') {
                    has_default = true;
                }
                (
                    head.trim_end_matches('?').to_string(),
                    decl[pos + 1..].trim().to_string(),
                )
            }
            None => (decl.trim_end_matches('?').to_string(), String::new()),
        };
        // A Kotlin parameter may carry the "vararg" marker in front of its
        // name. It collects the rest of the arguments, and it is an array at
        // runtime, so it travels as the comma separated text of its values.
        let mut vararg = false;
        if let Some(stripped) = name.strip_prefix("vararg ") {
            name = stripped.trim().to_string();
            vararg = true;
        }
        if name.is_empty() {
            continue;
        }
        let raw_type = if vararg {
            format!("{}[]", raw_type)
        } else {
            raw_type
        };
        let mut param = if raw_type.is_empty() {
            Param::untyped(&name)
        } else {
            Param::typed(&name, &raw_type)
        };
        param.rest = rest || vararg;
        param.keyword_only = keyword_only || catch_all;
        param.catch_all = catch_all;
        param.has_default = has_default;
        out.push(param);
    }
    out
}

/// Parse an untyped parameter list (Lua): just names.
fn parse_untyped_params(params_str: &str) -> Vec<Param> {
    split_top_level(params_str)
        .iter()
        .filter_map(|s| {
            let name = s.trim().trim_start_matches("...").to_string();
            if name.is_empty() || name == "self" {
                return None;
            }
            Some(Param::untyped(&name))
        })
        .collect()
}
