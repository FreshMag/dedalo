//! `dedalo inspect`: a compact outline of a resolved dungeon. It is the
//! smallest useful feedback for an author or an agent: an outline instead of
//! a dump (spec 13.2).

use dedalo_core::Resolved;
use dedalo_core::model::{Document, Num};
use serde_json::{Value, json};

fn c(v: i64) -> String {
    Num(v).to_string()
}

/// The resolved data of one space, opening, element or note.
pub fn element(r: &Resolved, id: &str) -> Option<Value> {
    let found = r
        .spaces
        .iter()
        .find(|s| s.id == id)
        .map(serde_json::to_value)
        .or_else(|| {
            r.openings
                .iter()
                .find(|o| o.id == id)
                .map(serde_json::to_value)
        })
        .or_else(|| {
            r.elements
                .iter()
                .find(|e| e.id == id)
                .map(serde_json::to_value)
        })
        .or_else(|| {
            r.notes
                .iter()
                .find(|n| n.id == id)
                .map(serde_json::to_value)
        })?;
    found.ok()
}

pub fn json(r: &Resolved, doc: Option<&Document>) -> Value {
    let spaces: Vec<Value> = r
        .spaces
        .iter()
        .map(|s| {
            let elements: Vec<Value> = r.elements.iter().filter(|e| e.space.as_ref() == Some(&s.id)).map(|e| json!({ "id": e.id, "type": e.ty, "subtype": e.subtype })).collect();
            // The level is named only in a dungeon that has levels.
            let level = (!r.levels.is_empty()).then_some(&s.level);
            json!({ "id": s.id, "kind": s.kind, "name": s.name, "level": level, "bbox": s.bbox, "visibility": s.visibility, "elements": elements })
        })
        .collect();
    let connections: Vec<Value> =
        r.openings.iter().map(|o| json!({ "id": o.id, "type": o.ty, "from": o.from, "to": o.to, "secret": o.secret, "locked": o.locked, "implicit": o.implicit })).collect();
    json!({
        "id": r.id,
        "name": r.name,
        "bounds": r.bounds,
        "levels": r.levels,
        "spaces": spaces,
        "connections": connections,
        "counts": { "spaces": r.spaces.len(), "connections": r.openings.len(), "elements": r.elements.len(), "notes": r.notes.len() },
        "exports": doc.map(|d| d.dungeon.exports.iter().map(|e| e.name.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "entrance": r.reach.entrance,
        "unreachable": r.reach.unreachable,
    })
}

pub fn text(r: &Resolved, doc: Option<&Document>) -> String {
    let mut out = format!("dungeon {}", r.id);
    if let Some(name) = &r.name {
        out.push_str(&format!(" \"{name}\""));
    }
    if let Some(unit) = &r.grid_unit {
        out.push_str(&format!("  (1 square = {unit})"));
    }
    out.push_str(&format!(
        "\nbounds: x {} to {}, y {} to {}\n",
        c(r.bounds.x0),
        c(r.bounds.x1),
        c(r.bounds.y0),
        c(r.bounds.y1)
    ));
    if !r.levels.is_empty() {
        out.push_str(&format!(
            "\nlevels ({}), side by side on the map:\n",
            r.levels.len()
        ));
        for level in &r.levels {
            let name = level
                .name
                .as_ref()
                .map(|n| format!(" \"{n}\""))
                .unwrap_or_default();
            out.push_str(&format!(
                "  level {}{name}: x {} to {}, y {} to {}\n",
                level.id,
                c(level.bounds.x0),
                c(level.bounds.x1),
                c(level.bounds.y0),
                c(level.bounds.y1)
            ));
        }
    }
    out.push_str(&format!("\nspaces ({}):\n", r.spaces.len()));
    for s in &r.spaces {
        let kind = format!("{:?}", s.kind).to_lowercase();
        let name = s
            .name
            .as_ref()
            .map(|n| format!(" \"{n}\""))
            .unwrap_or_default();
        let shape = if s.circle.is_some() { "circle, " } else { "" };
        let mut hidden = if s.visibility == dedalo_core::model::Visibility::Public {
            String::new()
        } else {
            format!("  [{}]", s.visibility.as_str())
        };
        if !r.levels.is_empty() {
            hidden.push_str(&format!("  (level {})", s.level));
        }
        out.push_str(&format!(
            "  {kind} {}{name}: {shape}{} by {} at x {} to {}, y {} to {}{hidden}\n",
            s.id,
            c(s.bbox.w()),
            c(s.bbox.h()),
            c(s.bbox.x0),
            c(s.bbox.x1),
            c(s.bbox.y0),
            c(s.bbox.y1)
        ));
        for e in r
            .elements
            .iter()
            .filter(|e| e.space.as_ref() == Some(&s.id))
        {
            let subtype = e
                .subtype
                .as_ref()
                .map(|t| format!(".{t}"))
                .unwrap_or_default();
            let place = match &e.contained_in {
                Some(holder) => format!("inside {holder}"),
                None => format!("at map cell [{}, {}]", c(e.bbox.x0), c(e.bbox.y0)),
            };
            out.push_str(&format!(
                "    {} ({}{subtype}) {place}  [{}]\n",
                e.id,
                e.ty,
                e.visibility.as_str()
            ));
        }
    }
    out.push_str(&format!("\nconnections ({}):\n", r.openings.len()));
    for o in &r.openings {
        let mut flags = Vec::new();
        if o.secret {
            flags.push("secret");
        }
        if o.locked {
            flags.push("locked");
        }
        if o.implicit {
            flags.push("corridor end");
        }
        if o.stub.is_some() {
            flags.push("stub passage");
        }
        let place = match (o.segment, o.cell) {
            (Some([a, b]), _) => {
                format!("from [{}, {}] to [{}, {}]", c(a.x), c(a.y), c(b.x), c(b.y))
            }
            (None, Some(cell)) => format!("at map cell [{}, {}]", c(cell.x0), c(cell.y0)),
            _ => String::new(),
        };
        let flags = if flags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", flags.join(", "))
        };
        out.push_str(&format!(
            "  {} ({}): {} -> {} {place}{flags}\n",
            o.id,
            o.ty,
            o.from,
            o.to.as_deref().unwrap_or("-")
        ));
    }
    out.push_str(&format!(
        "\nelements: {}   notes: {}",
        r.elements.len(),
        r.notes.len()
    ));
    if let Some(doc) = doc {
        let names: Vec<&str> = doc
            .dungeon
            .exports
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        out.push_str(&format!(
            "   exports: {}",
            if names.is_empty() {
                "(none)".to_string()
            } else {
                names.join(", ")
            }
        ));
    }
    out.push_str(&format!(
        "\nentrance: {}   unreachable: {}\n",
        r.reach.entrance.as_deref().unwrap_or("(none)"),
        if r.reach.unreachable.is_empty() {
            "none".to_string()
        } else {
            r.reach.unreachable.join(", ")
        }
    ));
    out
}
