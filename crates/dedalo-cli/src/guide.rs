//! The language reference, one topic at a time (spec 13.4 and 13.6).
//!
//! `dedalo guide`, the MCP `dedalo_guide` tool, its resources and the
//! exported agent skill all read these topics, so they cannot drift apart.

use dedalo_core::Registry;
use dedalo_core::catalog::Catalog;
use dedalo_core::model::Code;

const QUICKSTART: &str = include_str!("../../../docs/guide/quickstart.md");
const SYNTAX: &str = include_str!("../../../docs/guide/syntax.md");
const LAYOUT: &str = include_str!("../../../docs/guide/layout.md");

/// The complete examples, by name.
pub const EXAMPLES: &[(&str, &str, &str)] = &[
    (
        "forgotten_crypt",
        "Five rooms, a secret door, a corridor, traps, a chest and notes",
        include_str!("../../../tests/corpus/forgotten_crypt/input.dedalo"),
    ),
    (
        "four_shapes",
        "A rectangle, a circle, a square and a polygon; stubs, path corridors, stairs",
        include_str!("../../../tests/corpus/four_shapes/input.dedalo"),
    ),
    (
        "rotated",
        "Room rotation, automatic placement, conditional visibility, a custom legend",
        include_str!("../../../tests/corpus/rotated/input.dedalo"),
    ),
];

/// The topics, with a one-line description each.
pub fn topics(cat: &Catalog) -> Vec<(String, String)> {
    let mut out = vec![
        (
            "quickstart".to_string(),
            "The language in one page and the working loop".to_string(),
        ),
        (
            "syntax".to_string(),
            "Values, blocks, IDs, connections, visibility, exports".to_string(),
        ),
        (
            "layout".to_string(),
            "Coordinates, shapes, room placement, doors, corridors, elements".to_string(),
        ),
        (
            "catalog".to_string(),
            "Element types, subtypes and categories".to_string(),
        ),
        ("errors".to_string(), "Every diagnostic code".to_string()),
        ("examples".to_string(), "The complete examples".to_string()),
    ];
    out.extend(cat.types.keys().map(|t| {
        (
            format!("catalog/{t}"),
            format!("The `{t}` type and its properties"),
        )
    }));
    out.extend(
        Code::ALL
            .iter()
            .map(|c| (format!("errors/{}", c.as_str()), c.summary().to_string())),
    );
    out.extend(
        EXAMPLES
            .iter()
            .map(|(n, d, _)| (format!("examples/{n}"), d.to_string())),
    );
    out
}

/// The catalog as a Markdown table.
pub fn catalog_table(cat: &Catalog) -> String {
    let mut out = String::from(
        "# Dedalo catalog\n\nThe class is the block keyword: `feature` or `entity`. Use `type = custom` for anything else, with `properties { symbol = \"Al\", label = \"Altar of bones\" }`.\n\n| Type | Class | Category | Default visibility | Subtypes |\n| --- | --- | --- | --- | --- |\n",
    );
    for (name, t) in &cat.types {
        let class = format!("{:?}", t.class).to_lowercase();
        let subtypes = if t.subtypes.is_empty() {
            "free".to_string()
        } else {
            t.subtypes.join(", ")
        };
        out.push_str(&format!(
            "| `{name}` | {class} | {} | {} | {subtypes} |\n",
            t.category, t.default_visibility
        ));
    }
    out.push_str("\nRead `catalog/<type>` for the properties of one type.\n");
    out
}

/// One catalog type as Markdown.
pub fn catalog_type(cat: &Catalog, name: &str) -> Option<String> {
    let t = cat.get(name)?;
    let class = match t.class {
        dedalo_core::catalog::Class::Feature => "feature",
        dedalo_core::catalog::Class::Entity => "entity",
        dedalo_core::catalog::Class::Either => "feature or entity",
    };
    let mut out = format!(
        "# `{name}`: {}\n\nWritten as `{class}`. Category `{}`, visible to `{}` by default, footprint {} by {} cells{}.\n",
        t.label,
        t.category,
        t.default_visibility,
        t.footprint[0],
        t.footprint[1],
        if t.blocking { ", blocking" } else { "" }
    );
    if !t.subtypes.is_empty() {
        out.push_str(&format!(
            "\nSubtypes: {}.\n",
            t.subtypes
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("\n| Property | Type | Notes |\n| --- | --- | --- |\n");
    for (prop, spec) in t.properties.iter().chain(&cat.common.properties) {
        let mut notes = Vec::new();
        if let Some(values) = &spec.values {
            notes.push(values.join(" or "));
        }
        match (spec.min, spec.max) {
            (Some(a), Some(b)) => notes.push(format!("{a} to {b}")),
            (Some(a), None) => notes.push(format!("at least {a}")),
            _ => {}
        }
        if let Some(doc) = &spec.doc {
            notes.push(doc.clone());
        }
        let ty = match (spec.ty, spec.of) {
            (dedalo_core::catalog::PropType::List, Some(of)) => {
                format!("list of {}", of.describe())
            }
            (ty, _) => ty.describe().to_string(),
        };
        out.push_str(&format!("| `{prop}` | {ty} | {} |\n", notes.join("; ")));
    }
    out.push_str("\nNames starting with `x_` are always accepted as custom data.\n\n```hcl\n");
    out.push_str(&format!(
        "{class_kw} \"my_{name}\" {{\n    type = {name}\n{}    position = [1, 1]\n}}\n```\n",
        t.subtypes
            .first()
            .map(|s| format!("    subtype = {s}\n"))
            .unwrap_or_default(),
        class_kw = if t.class == dedalo_core::catalog::Class::Entity {
            "entity"
        } else {
            "feature"
        }
    ));
    Some(out)
}

/// The text of a topic.
pub fn topic(name: &str, reg: &Registry) -> Result<String, String> {
    let cat = &reg.catalog;
    Ok(match name {
        "quickstart" => QUICKSTART.to_string(),
        "syntax" => SYNTAX.to_string(),
        "layout" => LAYOUT.to_string(),
        "catalog" => catalog_table(cat),
        "errors" => {
            let mut out = String::from(
                "# Dedalo diagnostic codes\n\nRead `errors/<CODE>` for causes and a worked fix.\n\n",
            );
            for c in Code::ALL {
                out.push_str(&format!(
                    "- `{}` ({:?}): {}\n",
                    c.as_str(),
                    c.severity(),
                    c.summary()
                ));
            }
            out
        }
        "examples" => {
            let mut out = String::from("# Dedalo examples\n\n");
            for (n, d, _) in EXAMPLES {
                out.push_str(&format!("- `examples/{n}`: {d}\n"));
            }
            out
        }
        t if t.starts_with("catalog/") => catalog_type(cat, &t[8..])
            .ok_or_else(|| format!("`{}` is not a catalog type", &t[8..]))?,
        t if t.starts_with("errors/") => match Code::parse(&t[7..].to_uppercase()) {
            Some(c) => format!(
                "# {}\n\n{:?}, {} stage. {}\n\n{}\n",
                c.as_str(),
                c.severity(),
                c.stage(),
                c.summary(),
                c.explain()
            ),
            None => return Err(format!("`{}` is not a diagnostic code", &t[7..])),
        },
        t if t.starts_with("examples/") => match EXAMPLES.iter().find(|(n, _, _)| *n == &t[9..]) {
            Some((n, d, text)) => format!(
                "# Example `{n}`\n\n{d}.\n\n```hcl\n{}```\n",
                text.replace("\r\n", "\n")
            ),
            None => return Err(format!("`{}` is not an example", &t[9..])),
        },
        other => {
            let names: Vec<String> = topics(cat)
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| !n.contains('/'))
                .collect();
            return Err(format!(
                "`{other}` is not a guide topic; the topics are {}, and catalog/<type>, errors/<CODE>, examples/<name>",
                names.join(", ")
            ));
        }
    })
}

/// Writes the agent skill: `SKILL.md` and its reference files (spec 13.6).
pub fn export_skill(dir: &std::path::Path, reg: &Registry) -> std::io::Result<Vec<String>> {
    let refs = dir.join("references");
    std::fs::create_dir_all(&refs)?;
    let mut written = Vec::new();
    let skill = format!(
        "---\nname: dedalo\ndescription: Author, validate, edit and render tabletop-RPG dungeon maps with the Dedalo DSL and the `dedalo` command or MCP server. Use when asked to design a dungeon, draw a battle map, or change a `.dedalo` file.\n---\n\n# Dedalo\n\n{}\n\n## Working loop\n\n1. Read `references/quickstart.md` once.\n2. Write the file, then run `dedalo validate FILE --json` (MCP: `dedalo_validate`).\n3. Apply the `fix` of each diagnostic: `dedalo validate FILE --fix`, or send it to `dedalo edit FILE --ops -` (MCP: `dedalo_edit`).\n4. Read the result as text: `dedalo inspect FILE --text-map` (MCP: `dedalo_inspect` with `view = text_map`).\n5. Render: `dedalo render FILE --preset player --format png` (MCP: `dedalo_render`).\n6. Make further changes with small edit operations addressed by element ID rather than rewriting the file.\n\n## References\n\n- `references/quickstart.md`: the language in one page\n- `references/syntax.md`: values, blocks, connections, visibility, exports\n- `references/layout.md`: coordinates, shapes, placement, doors, corridors\n- `references/catalog.md`: element types; `references/catalog/<type>.md` for properties\n- `references/errors.md`: diagnostic codes and fixes\n- `references/examples/<name>.dedalo`: complete examples\n",
        "Dedalo compiles a text description of a dungeon into a map: SVG, PNG, PDF or a text map. Prefer relative room placement (`north_of`), wall and anchor placement for elements, and edit operations over regenerating files."
    );
    std::fs::write(dir.join("SKILL.md"), skill)?;
    written.push("SKILL.md".to_string());
    for name in ["quickstart", "syntax", "layout", "catalog"] {
        std::fs::write(
            refs.join(format!("{name}.md")),
            topic(name, reg).expect("a built-in topic"),
        )?;
        written.push(format!("references/{name}.md"));
    }
    let mut errors = topic("errors", reg).expect("a built-in topic");
    for c in Code::ALL {
        errors.push_str(&format!("\n## {}\n\n{}\n", c.as_str(), c.explain()));
    }
    std::fs::write(refs.join("errors.md"), errors)?;
    written.push("references/errors.md".to_string());
    std::fs::create_dir_all(refs.join("catalog"))?;
    for t in reg.catalog.types.keys() {
        std::fs::write(
            refs.join("catalog").join(format!("{t}.md")),
            catalog_type(&reg.catalog, t).expect("a catalog type"),
        )?;
        written.push(format!("references/catalog/{t}.md"));
    }
    std::fs::create_dir_all(refs.join("examples"))?;
    for (n, _, text) in EXAMPLES {
        std::fs::write(
            refs.join("examples").join(format!("{n}.dedalo")),
            text.replace("\r\n", "\n"),
        )?;
        written.push(format!("references/examples/{n}.dedalo"));
    }
    // The reference pages of the website list the files from here.
    let listed: Vec<&str> = written
        .iter()
        .filter_map(|f| f.strip_prefix("references/"))
        .collect();
    let index = serde_json::to_string_pretty(&listed).unwrap_or_default();
    std::fs::write(
        refs.join("index.json"),
        format!(
            "{index}
"
        ),
    )?;
    written.push("references/index.json".to_string());
    Ok(written)
}

/// `llms.txt`: the reference as one plain Markdown file for the website.
pub fn llms_txt(reg: &Registry) -> String {
    let mut out = String::from(
        "# Dedalo\n\n> Dedalo compiles a text description of a tabletop-RPG dungeon into a map. One Rust core powers a command-line binary, a static web editor and an MCP server.\n\n",
    );
    for name in ["quickstart", "syntax", "layout", "catalog", "errors"] {
        out.push_str(
            &topic(name, reg)
                .expect("a built-in topic")
                .replace("\n# ", "\n## "),
        );
        out.push('\n');
    }
    out
}
