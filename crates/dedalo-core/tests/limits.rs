//! Input limits bound resource use (spec 10.3): oversized geometry is
//! reported, never computed.

use dedalo_core::{ExportSpec, Input, Preset, Registry};

fn codes(source: &str) -> Vec<(String, String)> {
    let compiled = dedalo_core::compile(Input::Dsl(source), &Registry::builtin());
    compiled
        .diagnostics
        .iter()
        .map(|d| (d.code.as_str().to_string(), d.message.clone()))
        .collect()
}

#[test]
fn coordinates_and_shapes_have_a_supported_range() {
    let far = codes(
        "dungeon \"d\" {\n room \"a\" {\n shape = square(4)\n layout { position = [400000, 0] }\n }\n}\n",
    );
    assert_eq!(far.len(), 1, "{far:#?}");
    assert_eq!(far[0].0, "INVALID_VALUE");
    assert!(
        far[0]
            .1
            .contains("outside the supported range of -100000 to 100000 cells"),
        "{}",
        far[0].1
    );

    let huge = codes("dungeon \"d\" {\n room \"a\" { shape = rectangle(5000, 5) }\n}\n");
    assert_eq!(huge[0].0, "INVALID_SHAPE");
    assert!(
        huge[0].1.contains("the largest supported is 1000 by 1000"),
        "{}",
        huge[0].1
    );

    let wide = codes(
        "dungeon \"d\" {\n room \"a\" {\n shape = square(4)\n feature \"lake\" {\n type = terrain\n position = [0, 0]\n size = [500, 1]\n }\n }\n}\n",
    );
    assert!(
        wide.iter()
            .any(|(code, message)| code == "INVALID_VALUE" && message.contains("`size`")),
        "{wide:#?}"
    );
}

#[test]
fn a_chain_of_rooms_cannot_walk_off_the_map() {
    // Each room sits 90,000 cells east of the one before.
    let mut source = String::from("dungeon \"d\" {\n room \"r0\" { shape = square(4) }\n");
    for i in 1..4 {
        source.push_str(&format!(" room \"r{i}\" {{\n shape = square(4)\n layout {{\n east_of = \"r{}\"\n gap = 90000\n }}\n }}\n", i - 1));
    }
    source.push_str("}\n");
    let found = codes(&source);
    assert!(
        found.iter().any(|(code, message)| code == "INVALID_VALUE"
            && message.contains("outside the supported area")),
        "{found:#?}"
    );
}

#[test]
fn a_sparse_map_gets_a_note_instead_of_a_huge_text_map() {
    let reg = Registry::builtin();
    let source = "dungeon \"d\" {\n room \"a\" {\n shape = square(4)\n layout { position = [0, 0] }\n }\n room \"b\" {\n shape = square(4)\n layout { position = [50000, 0] }\n }\n}\n";
    let compiled = dedalo_core::compile(Input::Dsl(source), &reg);
    let mut spec = ExportSpec::preset(Preset::Dm);
    spec.format = "txt".to_string();
    let text =
        String::from_utf8(dedalo_core::render(&compiled, &spec, &reg).unwrap().bytes).unwrap();
    assert_eq!(
        text,
        "(the map is 50004 by 4 cells; a text map is limited to 1000 by 1000)\n"
    );
    // The SVG has no such limit: its size follows the content, not the extent.
    spec.format = "svg".to_string();
    assert!(
        dedalo_core::render(&compiled, &spec, &reg)
            .unwrap()
            .bytes
            .len()
            < 20_000
    );
}
