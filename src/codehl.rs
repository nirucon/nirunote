//! Cheap regex syntax highlighting for fenced code in the Preview, a port of
//! NIRUNOTE 0.7's `highlight.py`: one pattern per language classifies
//! comments, strings, numbers and words; the output is escaped HTML with
//! `<span class="tX">` tokens. Unknown languages and very large blocks are
//! plain text.
use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

pub const MAX_HIGHLIGHT_CHARS: usize = 60000;

const C_COMMENT: &str = r"//[^\n]*|/\*.*?\*/";
const HASH_COMMENT: &str = r"(?<![\w$#{])#[^\n]*";
const DQ: &str = r#""(?:\\.|[^"\\\n])*""#;
const SQ: &str = r"'(?:\\.|[^'\\\n])*'";
const NUMBER: &str =
    r"\b(?:0[xX][0-9a-fA-F_]+|0[bB][01_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)\b";

struct Spec {
    comment: Option<String>,
    strings: Option<String>,
    extra: Vec<(&'static str, String)>,
    keywords: &'static str,
    builtins: &'static str,
    command_words: bool,
    nocase: bool,
    words: bool,
    lines: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            comment: None,
            strings: None,
            extra: Vec::new(),
            keywords: "",
            builtins: "",
            command_words: false,
            nocase: false,
            words: true,
            lines: false,
        }
    }
}

fn spec(name: &str) -> Option<Spec> {
    let s = |x: &str| Some(x.to_string());
    Some(match name {
        "python" => Spec {
            comment: s(r"#[^\n]*"),
            strings: Some(format!(
                r#"[rRbBfFuU]{{0,2}}(?:"""[\s\S]*?"""|'''[\s\S]*?'''|{DQ}|{SQ})"#
            )),
            extra: vec![("tp", r"@[\w.]+".into())],
            keywords: "and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case",
            builtins: "True False None self cls print len range str int float list dict set tuple bool open super isinstance type object Exception ValueError KeyError",
            ..Default::default()
        },
        "shell" => Spec {
            comment: s(HASH_COMMENT),
            strings: Some(format!("{DQ}|'[^']*'")),
            extra: vec![(
                "tb",
                r"\$\{[^}\n]*\}|\$\([^)\n]*\)|\$[A-Za-z_]\w*|\$[0-9@#?*!$-]".into(),
            )],
            keywords: "if then else elif fi for while until do done case esac in function return local export readonly set unset source exit select time",
            command_words: true,
            ..Default::default()
        },
        "fish" => Spec {
            comment: s(HASH_COMMENT),
            strings: Some(format!("{DQ}|'[^']*'")),
            extra: vec![("tb", r"\$[A-Za-z_]\w*".into())],
            keywords: "if else end for while in function return switch case begin and or not set functions source exit break continue",
            command_words: true,
            ..Default::default()
        },
        "c" => Spec {
            comment: s(C_COMMENT),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![("tp", r"^[ \t]*#[ \t]*\w+[^\n]*".into())],
            keywords: "if else for while do switch case default break continue return goto struct union enum typedef sizeof extern inline volatile register class namespace template typename public private protected virtual override new delete using try catch throw this nullptr true false constexpr noexcept operator",
            builtins: "int long short char float double void bool unsigned signed size_t ssize_t uint8_t uint16_t uint32_t uint64_t int8_t int16_t int32_t int64_t auto const static NULL std string vector",
            ..Default::default()
        },
        "rust" => Spec {
            comment: s(C_COMMENT),
            strings: Some(format!(r##"r#*"[\s\S]*?"#*|b?{DQ}|b?'(?:\\.|[^\\'\n])'"##)),
            extra: vec![
                ("tp", r"#!?\[[^\]\n]*\]".into()),
                ("tf", r"\b\w+!".into()),
                ("tb", r"'[a-z_]\w*\b".into()),
            ],
            keywords: "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while",
            builtins: "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec Option Some None Result Ok Err Box",
            ..Default::default()
        },
        "go" => Spec {
            comment: s(C_COMMENT),
            strings: Some(format!("{DQ}|`[^`]*`|{SQ}")),
            keywords: "break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var true false nil",
            builtins: "int int8 int16 int32 int64 uint uint8 uint16 uint32 uint64 float32 float64 string bool byte rune error make len cap append new panic",
            ..Default::default()
        },
        "js" => Spec {
            comment: s(C_COMMENT),
            strings: Some(format!(r"{DQ}|{SQ}|`(?:\\.|[^`\\])*`")),
            keywords: "break case catch class const continue debugger default delete do else export extends finally for function if import in instanceof let new return super switch this throw try typeof var void while with yield async await of from as interface type enum implements readonly",
            builtins: "true false null undefined NaN Infinity console window document Math JSON Promise Array Object String Number Boolean string number boolean any unknown never",
            ..Default::default()
        },
        "java" => Spec {
            comment: s(C_COMMENT),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![("tp", r"@\w+".into())],
            keywords: "abstract break case catch class continue default do else enum extends final finally for if implements import instanceof interface new package private protected public return static super switch this throw throws try while val var fun object when true false null",
            builtins: "int long short byte char float double boolean void String",
            ..Default::default()
        },
        "lua" => Spec {
            comment: s(r"--\[\[[\s\S]*?\]\]|--[^\n]*"),
            strings: Some(format!(r"{DQ}|{SQ}|\[\[[\s\S]*?\]\]")),
            keywords: "and break do else elseif end false for function goto if in local nil not or repeat return then true until while",
            builtins: "print pairs ipairs require table string math os io type tostring",
            ..Default::default()
        },
        "sql" => Spec {
            comment: s(r"--[^\n]*|/\*.*?\*/"),
            strings: Some(format!("{SQ}|{DQ}")),
            keywords: "select from where and or not insert into values update set delete create table drop alter add index primary key foreign references join left right inner outer on group by order having limit offset as distinct union all null is in like between case when then else end begin commit rollback",
            builtins: "integer int text varchar real blob boolean date count sum avg min max",
            nocase: true,
            ..Default::default()
        },
        "json" => Spec {
            extra: vec![
                ("tf", r#""(?:\\.|[^"\\\n])*"(?=\s*:)"#.into()),
                ("ts", DQ.into()),
            ],
            keywords: "true false null",
            ..Default::default()
        },
        "toml" => Spec {
            comment: s(r"#[^\n]*"),
            strings: Some(format!(r#""""[\s\S]*?"""|'''[\s\S]*?'''|{DQ}|'[^'\n]*'"#)),
            extra: vec![
                ("tk", r"^[ \t]*\[\[?[^\]\n]*\]\]?".into()),
                ("tf", r#"^[ \t]*[\w.\-"']+(?=[ \t]*=)"#.into()),
            ],
            keywords: "true false",
            ..Default::default()
        },
        "ini" => Spec {
            comment: s(r"^[ \t]*[#;][^\n]*"),
            strings: s(DQ),
            extra: vec![
                ("tk", r"^[ \t]*\[[^\]\n]*\]".into()),
                ("tf", r"^[ \t]*[\w.\-]+(?=[ \t]*[=:])".into()),
            ],
            keywords: "true false yes no on off",
            ..Default::default()
        },
        "yaml" => Spec {
            comment: s(r"(?<!\S)#[^\n]*"),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![
                ("tf", r"^[ \t]*(?:- )?[\w.\-/ ]+(?=:(?:[ \t]|$))".into()),
                ("tp", r"[&*][\w-]+|!!?\w+".into()),
            ],
            keywords: "true false null yes no on off",
            ..Default::default()
        },
        "css" => Spec {
            comment: s(r"/\*.*?\*/"),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![
                ("tn", r"#[0-9a-fA-F]{3,8}\b".into()),
                ("tf", r"[\w-]+(?=\s*:[^:])".into()),
                ("tk", r"@[\w-]+".into()),
            ],
            keywords: "important inherit initial none auto",
            ..Default::default()
        },
        "html" => Spec {
            comment: s(r"<!--[\s\S]*?-->"),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![
                ("tk", r"</?[\w:-]+|/?>".into()),
                ("tf", r"\b[\w:-]+(?==)".into()),
                ("tb", r"&#?\w+;".into()),
            ],
            words: false,
            ..Default::default()
        },
        "make" => Spec {
            comment: s(r"#[^\n]*"),
            strings: Some(format!("{DQ}|{SQ}")),
            extra: vec![
                ("tb", r"\$[({][^)}\n]*[)}]|\$[@<^*?]".into()),
                ("tf", r"^[\w.%/ -]+(?=:)".into()),
            ],
            keywords: "ifeq ifneq ifdef ifndef else endif include define endef export",
            ..Default::default()
        },
        "diff" => Spec {
            lines: true,
            ..Default::default()
        },
        _ => return None,
    })
}

const COMMAND_KEEP: &str =
    "sudo exec time env nohup command builtin then do else elif ! if while until";

fn alias(lang: &str) -> &str {
    match lang {
        "py" | "python3" => "python",
        "sh" | "bash" | "zsh" | "ksh" | "console" | "shell-session" | "shellsession"
        | "terminal" => "shell",
        "cpp" | "c++" | "cc" | "h" | "hpp" | "cxx" | "objc" => "c",
        "cs" | "csharp" | "kotlin" | "kt" | "swift" | "scala" => "java",
        "rs" => "rust",
        "golang" => "go",
        "javascript" | "jsx" | "ts" | "tsx" | "typescript" | "mjs" => "js",
        "jsonc" | "json5" => "json",
        "yml" => "yaml",
        "conf" | "cfg" | "desktop" | "systemd" | "properties" | "editorconfig" => "ini",
        "xml" | "svg" | "xhtml" | "vue" => "html",
        "scss" | "less" => "css",
        "makefile" | "mk" => "make",
        "patch" | "udiff" => "diff",
        "postgres" | "sqlite" | "mysql" => "sql",
        other => other,
    }
}

pub fn language(lang: &str) -> Option<String> {
    let l = lang
        .trim()
        .to_lowercase()
        .trim_start_matches(['.', '{'])
        .trim_end_matches('}')
        .to_string();
    let name = alias(&l).to_string();
    spec(&name).map(|_| name)
}

struct Compiled {
    re: Option<Regex>,
    groups: Vec<String>,
    spec: Spec,
}

fn compiled(name: &str) -> &'static Compiled {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static Compiled>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut g = cache.lock().unwrap();
    if let Some(c) = g.get(name) {
        return c;
    }
    let spec = spec(name).unwrap();
    let mut parts = Vec::new();
    let mut groups = Vec::new();
    if let Some(c) = &spec.comment {
        parts.push(format!("(?P<tc>{c})"));
        groups.push("tc".to_string());
    }
    if let Some(s) = &spec.strings {
        parts.push(format!("(?P<ts>{s})"));
        groups.push("ts".to_string());
    }
    for (i, (cls, pat)) in spec.extra.iter().enumerate() {
        let g = format!("x{i}_{cls}");
        parts.push(format!("(?P<{g}>{pat})"));
        groups.push(g);
    }
    if spec.words {
        if spec.command_words {
            parts.push(r"(?P<sep>\n|&&|\|\||[|;&(`]|\$\()".into());
            groups.push("sep".into());
            parts.push(r"(?P<w>[\w./~+-][\w./~+:=@%,-]*)".into());
            groups.push("w".into());
        } else {
            parts.push(format!("(?P<tn>{NUMBER})"));
            groups.push("tn".into());
            parts.push(r"(?P<w>[A-Za-z_][\w]*)".into());
            groups.push("w".into());
        }
    }
    let flags = if spec.nocase { "(?smi)" } else { "(?sm)" };
    let re = if spec.lines {
        None
    } else {
        Regex::new(&format!("{flags}{}", parts.join("|"))).ok()
    };
    let c: &'static Compiled = Box::leak(Box::new(Compiled { re, groups, spec }));
    g.insert(name.to_string(), c);
    c
}

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

fn span(cls: &str, text: &str) -> String {
    format!("<span class=\"{cls}\">{}</span>", esc(text))
}

fn diff(code: &str) -> String {
    code.split('\n')
        .map(|line| {
            if line.starts_with("+++")
                || line.starts_with("---")
                || line.starts_with("diff ")
                || line.starts_with("index ")
            {
                span("tk", line)
            } else if line.starts_with("@@") {
                span("th", line)
            } else if line.starts_with('+') {
                span("ta", line)
            } else if line.starts_with('-') {
                span("td", line)
            } else {
                esc(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_word(list: &str, w: &str) -> bool {
    list.split_whitespace().any(|x| x == w)
}

/// Escaped HTML for `code`, highlighted when `lang` is known and small.
pub fn highlight_code(code: &str, lang: &str) -> String {
    let Some(name) = language(lang) else {
        return esc(code);
    };
    if code.chars().count() > MAX_HIGHLIGHT_CHARS {
        return esc(code);
    }
    let c = compiled(&name);
    if c.spec.lines {
        return diff(code);
    }
    let Some(re) = &c.re else {
        return esc(code);
    };
    let spec = &c.spec;
    let mut out = String::with_capacity(code.len() * 2);
    let mut pos = 0;
    let mut at_command = true;
    for caps in re.captures_iter(code) {
        let Ok(caps) = caps else {
            break;
        };
        let m = caps.get(0).unwrap();
        let (start, end) = (m.start(), m.end());
        if start > pos {
            out.push_str(&esc(&code[pos..start]));
        }
        let text = m.as_str();
        pos = end;
        let kind = c
            .groups
            .iter()
            .rev()
            .find(|g| caps.name(g).is_some())
            .cloned()
            .unwrap_or_default();
        if kind == "sep" {
            at_command = true;
            out.push_str(&esc(text));
            continue;
        }
        if kind == "w" {
            let word = if spec.nocase {
                text.to_lowercase()
            } else {
                text.to_string()
            };
            if has_word(spec.keywords, &word) {
                out.push_str(&span("tk", text));
                at_command = spec.command_words && has_word(COMMAND_KEEP, &word);
            } else if spec.command_words {
                if text.contains('=') && at_command && !text.starts_with('-') {
                    out.push_str(&span("tb", text));
                } else if at_command && !text.chars().next().is_some_and(|ch| ch.is_numeric()) {
                    out.push_str(&span("tf", text));
                    at_command = has_word(COMMAND_KEEP, text);
                } else {
                    out.push_str(&esc(text));
                }
            } else if has_word(spec.builtins, &word) {
                out.push_str(&span("tb", text));
            } else if code[end..].starts_with('(') || (code[end..].starts_with(" (") && name == "c")
            {
                out.push_str(&span("tf", text));
            } else {
                out.push_str(&esc(text));
            }
            continue;
        }
        let cls = if kind.starts_with('x') {
            kind.rsplit('_').next().unwrap_or("").to_string()
        } else {
            kind
        };
        out.push_str(&span(&cls, text));
        if spec.command_words && cls != "tc" {
            at_command = false;
        }
    }
    out.push_str(&esc(&code[pos..]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_and_shell_tokens() {
        let out = highlight_code("def f():\n    return \"x\"  # c", "python");
        assert!(out.contains("<span class=\"tk\">def</span>"), "{out}");
        assert!(out.contains("<span class=\"ts\">\"x\"</span>"), "{out}");
        assert!(out.contains("<span class=\"tc\"># c</span>"), "{out}");
        let sh = highlight_code("nirux manual --where x | grep -q y", "sh");
        assert!(sh.contains("<span class=\"tf\">nirux</span>"), "{sh}");
        assert!(sh.contains("<span class=\"tf\">grep</span>"), "{sh}");
        assert_eq!(highlight_code("<b>", "nope"), "&lt;b&gt;");
    }
}
