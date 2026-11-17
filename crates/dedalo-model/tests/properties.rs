//! The round-trip laws of spec 6.3 as properties over generated documents.
//!
//! The corpus checks the laws on examples; here documents are built at
//! random from every part of the model: levels, rooms of every shape,
//! corridors, connections, elements, notes, exports, free-form properties
//! and comments.

use std::collections::BTreeMap;

use dedalo_catalog::Catalog;
use dedalo_model::*;
use dedalo_model::{lower::lower, write::to_dsl};
use proptest::collection::{btree_map, vec};
use proptest::option::of;
use proptest::prelude::*;
use proptest::sample::select;

fn json(source: &str, cat: &Catalog) -> Result<Document, String> {
    let parsed = dedalo_syntax::parse(source);
    if !parsed.errors.is_empty() {
        return Err(format!("{:?}", parsed.errors));
    }
    let lowered = lower(&parsed.syntax(), source, cat);
    lowered
        .document
        .ok_or_else(|| format!("{:?}", lowered.diagnostics))
}

fn fmt(source: &str, cat: &Catalog) -> String {
    let parsed = dedalo_syntax::parse(source);
    let lowered = lower(&parsed.syntax(), source, cat);
    dedalo_syntax::fmt::format(&parsed.syntax(), &lowered.rewrites)
}

/// Where two documents differ, as lines of their JSON.
fn difference(wanted: &Document, got: &Document) -> String {
    let lines = |d: &Document| serde_json::to_string_pretty(d).unwrap();
    let (wanted, got) = (lines(wanted), lines(got));
    let mut out = String::from("the documents differ (- written, + read back):\n");
    let (mut a, mut b) = (wanted.lines().peekable(), got.lines().peekable());
    let mut shown = 0;
    while let (Some(x), Some(y)) = (a.peek(), b.peek()) {
        if x != y && shown < 12 {
            out.push_str(&format!("- {x}\n+ {y}\n"));
            shown += 1;
        }
        a.next();
        b.next();
    }
    out
}

// ----- generators ---------------------------------------------------------------

/// Text with the characters that need care when quoted.
fn text() -> impl Strategy<Value = String> {
    vec(
        select(vec![
            "a", "Z", "9", " ", "è", "ß", "猫", "\"", "\\", "'", "#", "//", "{", "}", "=", "$",
            "\t", "end", "-", ".",
        ]),
        1..8,
    )
    .prop_map(|parts| parts.concat())
}

/// Text that may run over several lines, as a note's text does.
fn long_text() -> impl Strategy<Value = String> {
    vec(text(), 1..4).prop_map(|lines| lines.join("\n"))
}

fn word() -> impl Strategy<Value = String> {
    select(vec![
        "stone",
        "wood",
        "iron",
        "north",
        "true_north",
        "a1",
        "door",
        "entrance",
        "x_y",
    ])
    .prop_map(str::to_string)
}

fn num() -> impl Strategy<Value = Num> {
    prop_oneof![
        (-40i64..40).prop_map(Num::int),
        (-40_000i64..40_000).prop_map(Num),
    ]
}

fn positive() -> impl Strategy<Value = Num> {
    prop_oneof![
        (1i64..12).prop_map(Num::int),
        (1000i64..12_000).prop_map(Num)
    ]
}

fn point() -> impl Strategy<Value = Point> {
    [num(), num()]
}

fn visibility() -> impl Strategy<Value = Option<Visibility>> {
    of(select(vec![
        Visibility::Public,
        Visibility::Dm,
        Visibility::Conditional,
        Visibility::Hidden,
    ]))
}

/// A free-form value, as an `x_` property holds.
fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(Value::Bool),
        num().prop_map(Value::Num),
        text().prop_map(Value::Str),
    ];
    leaf.prop_recursive(2, 8, 3, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..3).prop_map(Value::List),
            btree_map(
                select(vec!["k", "key_2", "z"]).prop_map(str::to_string),
                inner,
                1..3
            )
            .prop_map(Value::Map),
        ]
    })
}

/// A `properties` block: typed catalog properties and free `x_` ones.
fn properties(typed: Vec<(&'static str, BoxedStrategy<Value>)>) -> impl Strategy<Value = Props> {
    let typed: Vec<BoxedStrategy<Option<(String, Value)>>> = typed
        .into_iter()
        .map(|(name, strategy)| of(strategy.prop_map(move |v| (name.to_string(), v))).boxed())
        .collect();
    (
        typed,
        btree_map(
            select(vec!["x_note", "x_weight", "x_tags"]).prop_map(str::to_string),
            value(),
            0..2,
        ),
        of(text()),
    )
        .prop_map(|(typed, mut free, description)| {
            free.extend(typed.into_iter().flatten());
            if let Some(d) = description {
                free.insert("description".to_string(), Value::Str(d));
            }
            free
        })
}

fn shape() -> impl Strategy<Value = ShapeCall> {
    let call = |func: &str, args: Vec<Value>| ShapeCall {
        func: func.to_string(),
        args,
    };
    prop_oneof![
        positive().prop_map(move |s| call("square", vec![Value::Num(s)])),
        (positive(), positive())
            .prop_map(move |(w, h)| call("rectangle", vec![Value::Num(w), Value::Num(h)])),
        positive().prop_map(move |r| call("circle", vec![Value::Num(r)])),
        (positive(), positive())
            .prop_map(move |(w, h)| call("ellipse", vec![Value::Num(w), Value::Num(h)])),
        vec(point(), 3..6).prop_map(move |points| {
            let list = points
                .into_iter()
                .map(|p| Value::List(vec![Value::Num(p[0]), Value::Num(p[1])]))
                .collect();
            call("polygon", vec![Value::List(list)])
        }),
    ]
}

fn layout(rooms: Vec<String>) -> impl Strategy<Value = Layout> {
    let relative = (
        select(rooms),
        select(vec![
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ]),
        of(select(vec![Align::Start, Align::Center, Align::End])),
        of(num()),
        of(positive()),
    )
        .prop_map(|(target, direction, align, offset, gap)| Layout {
            adjacent_to: Some(target),
            direction: Some(direction),
            align,
            offset,
            gap,
            ..Layout::default()
        });
    prop_oneof![
        point().prop_map(|p| Layout {
            position: Some(p),
            ..Layout::default()
        }),
        relative,
        Just(Layout {
            mode: Some(LayoutMode::Automatic),
            ..Layout::default()
        }),
    ]
}

/// Comments on a node: lines above it and one on its own line.
fn comments() -> impl Strategy<Value = Comments> {
    let line = select(vec!["# a note", "# TODO: check", "// slashes", "# è", "#"])
        .prop_map(str::to_string);
    (vec(line.clone(), 0..3), of(line)).prop_map(|(leading, trailing)| Comments {
        leading,
        trailing,
        ..Comments::default()
    })
}

fn element(id: String) -> impl Strategy<Value = Element> {
    let kind = select(vec![
        (Class::Feature, "furniture", vec!["table", "chair", "altar"]),
        (Class::Feature, "lighting", vec!["torch", "brazier"]),
        (Class::Feature, "trap", vec!["pit", "pressure_plate"]),
        (Class::Feature, "terrain", vec!["water", "rubble"]),
        (Class::Feature, "custom", vec![]),
        (Class::Entity, "creature", vec![]),
        (Class::Entity, "npc", vec![]),
        (Class::Entity, "item", vec!["key", "treasure"]),
        (Class::Entity, "custom", vec![]),
    ]);
    let place = prop_oneof![
        point().prop_map(|p| (Some(p), None)),
        (
            select(vec![
                Direction::North,
                Direction::South,
                Direction::East,
                Direction::West
            ]),
            of(num())
        )
            .prop_map(|(wall, offset)| {
                (
                    None,
                    Some(ElementPlacement {
                        wall: Some(wall),
                        offset,
                        anchor: None,
                    }),
                )
            }),
        select(vec![Anchor::Center, Anchor::NorthEast, Anchor::South]).prop_map(|anchor| {
            (
                None,
                Some(ElementPlacement {
                    wall: None,
                    offset: None,
                    anchor: Some(anchor),
                }),
            )
        }),
    ];
    (
        kind,
        any::<prop::sample::Index>(),
        of(text()),
        place,
        of([positive(), positive()]),
        of(select(vec![0i64, 90, 180, 270])),
        of(select(vec![Facing::North, Facing::SouthWest, Facing::East])),
        visibility(),
        (
            of(text()),
            vec(word(), 0..3),
            properties(vec![]),
            comments(),
        ),
    )
        .prop_map(
            move |(
                (class, ty, subtypes),
                pick,
                name,
                (position, placement),
                size,
                rotation,
                facing,
                visibility,
                (reference, tags, properties, comments),
            )| {
                let mut e = Element::new(class, id.clone());
                e.ty = ty.to_string();
                // Some elements name a subtype, some leave it out.
                e.subtype = (!subtypes.is_empty() && pick.index(3) > 0)
                    .then(|| subtypes[pick.index(subtypes.len())].to_string());
                e.name = name;
                e.position = position;
                e.placement = placement;
                e.size = size;
                e.rotation = rotation;
                e.facing = facing;
                e.visibility = visibility;
                e.reveal_when = visibility
                    .filter(|v| *v == Visibility::Conditional)
                    .map(|_| "found".to_string());
                e.reference = reference;
                e.tags = tags;
                e.properties = properties;
                e.comments = comments;
                e
            },
        )
}

fn room(
    id: String,
    level: Option<String>,
    others: Vec<String>,
    elements: usize,
) -> impl Strategy<Value = Space> {
    let ids: Vec<String> = (0..elements).map(|k| format!("{id}_e{k}")).collect();
    let layout = if others.is_empty() {
        of(point().prop_map(|p| Layout {
            position: Some(p),
            ..Layout::default()
        }))
        .boxed()
    } else {
        of(layout(others)).boxed()
    };
    let typed = vec![
        ("wall", word().prop_map(Value::Str).boxed()),
        ("ceiling_height", positive().prop_map(Value::Num).boxed()),
    ];
    (
        of(text()),
        of(shape()),
        of(select(vec![0i64, 90, 180, 270])),
        layout,
        visibility(),
        vec(word(), 0..3),
        properties(typed),
        ids.into_iter().map(element).collect::<Vec<_>>(),
        comments(),
    )
        .prop_map(
            move |(
                name,
                shape,
                rotation,
                layout,
                visibility,
                tags,
                properties,
                elements,
                comments,
            )| {
                let mut s = Space::new(SpaceKind::Room, id.clone());
                s.level = level.clone();
                s.name = name;
                s.shape = shape;
                s.rotation = rotation;
                s.layout = layout;
                s.visibility = visibility;
                s.tags = tags;
                s.properties = properties;
                s.elements = elements;
                s.comments = comments;
                s
            },
        )
}

fn corridor(id: String, level: Option<String>, rooms: Vec<String>) -> impl Strategy<Value = Space> {
    (
        select(rooms.clone()),
        select(rooms),
        of(positive()),
        vec(point(), 0..4),
        of(positive()),
        of(num()),
        of(text()),
        comments(),
    )
        .prop_map(move |(a, b, width, path, length, offset, name, comments)| {
            let mut s = Space::new(SpaceKind::Corridor, id.clone());
            s.level = level.clone();
            s.connects = vec![a, b];
            s.width = width;
            s.path = path;
            s.length = length;
            s.offset = offset;
            s.name = name;
            s.comments = comments;
            s
        })
}

fn connection(id: String, spaces: Vec<String>) -> impl Strategy<Value = Connection> {
    let placement = of((
        of(select(vec![
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ])),
        of(0i64..6),
        of((0i64..360).prop_map(Num::int)),
        of(num()),
        of(positive()),
    )
        .prop_filter("a placement block sets something", |p| {
            p.0.is_some() || p.1.is_some() || p.2.is_some() || p.3.is_some() || p.4.is_some()
        })
        .prop_map(|(wall, edge, bearing, offset, width)| ConnectionPlacement {
            wall,
            edge,
            bearing,
            offset,
            width,
        }));
    let typed = vec![
        ("locked", any::<bool>().prop_map(Value::Bool).boxed()),
        (
            "lock_dc",
            (0i64..40).prop_map(|n| Value::Num(Num::int(n))).boxed(),
        ),
        ("material", word().prop_map(Value::Str).boxed()),
    ];
    (
        select(spaces.clone()),
        of(select(spaces)),
        select(vec![
            "door", "archway", "open", "stairs", "ladder", "trapdoor",
        ]),
        of(any::<bool>()),
        of(point()),
        of(point()),
        placement,
        visibility(),
        (of(text()), vec(word(), 0..2), properties(typed), comments()),
    )
        .prop_map(
            move |(
                from,
                to,
                ty,
                secret,
                position,
                to_position,
                placement,
                visibility,
                (name, tags, properties, comments),
            )| {
                Connection {
                    id: id.clone(),
                    name,
                    from,
                    to,
                    ty: ty.to_string(),
                    secret,
                    position,
                    to_position,
                    placement,
                    visibility,
                    reveal_when: None,
                    tags,
                    properties,
                    comments,
                }
            },
        )
}

fn export(name: String, levels: Vec<String>) -> impl Strategy<Value = Export> {
    let level = if levels.is_empty() {
        Just(None).boxed()
    } else {
        of(select(levels)).boxed()
    };
    (
        of(select(vec![Preset::Dm, Preset::Player, Preset::Clean])),
        of(select(vec!["svg", "png", "pdf", "txt", "uvtt.dd2vtt"]).prop_map(str::to_string)),
        of(select(vec![
            Resolution::Low,
            Resolution::Medium,
            Resolution::High,
        ])),
        of(16i64..512),
        of(select(vec![0i64, 90, 180, 270])),
        level,
        vec(word(), 0..2),
        of(select(vec![Bounds::Visible, Bounds::Full])),
        (
            of(select(vec![Page::Fit, Page::A4, Page::Letter])),
            of(select(vec![Orientation::Portrait, Orientation::Landscape])),
            of(select(vec![PrintScale::Fit, PrintScale::Tabletop])),
            btree_map(
                select(TOGGLES.to_vec()).prop_map(str::to_string),
                any::<bool>(),
                0..3,
            ),
            of(render()),
            of(legend()),
        ),
    )
        .prop_map(
            move |(
                preset,
                format,
                resolution,
                cell_px,
                rotation,
                level,
                revealed,
                bounds,
                (page, orientation, scale, visibility, render, legend),
            )| {
                Export {
                    name: name.clone(),
                    preset,
                    format,
                    resolution,
                    cell_px,
                    rotation,
                    level,
                    revealed,
                    include: Vec::new(),
                    exclude: Vec::new(),
                    bounds,
                    page,
                    orientation,
                    scale,
                    visibility,
                    legend,
                    render,
                    comments: Comments::default(),
                }
            },
        )
}

fn render() -> impl Strategy<Value = RenderOptions> {
    (
        of(select(vec!["classic", "hand_drawn.sepia"]).prop_map(str::to_string)),
        of(any::<bool>()),
        of(select(vec![TextMode::Outline, TextMode::Live])),
        of(any::<bool>()),
        of(select(vec![Language::En, Language::It])),
        btree_map(
            select(vec!["hand_drawn", "uvtt"]).prop_map(str::to_string),
            btree_map(
                select(vec!["wobble", "mode"]).prop_map(str::to_string),
                value(),
                1..3,
            ),
            0..2,
        ),
    )
        .prop_map(
            |(theme, title, text, room_ids, language, plugins)| RenderOptions {
                theme,
                title,
                text,
                room_ids,
                language,
                plugins,
            },
        )
}

fn legend() -> impl Strategy<Value = Legend> {
    (
        of(select(vec![
            LegendMode::Automatic,
            LegendMode::Custom,
            LegendMode::None,
        ])),
        of(select(vec![
            LegendPosition::Right,
            LegendPosition::Bottom,
            LegendPosition::Top,
        ])),
        of(text()),
        vec(
            (select(vec!["door", "trap", "lighting.torch"]), of(text())),
            0..3,
        ),
    )
        .prop_map(|(mode, position, title, entries)| Legend {
            mode,
            position,
            title,
            entries: entries
                .into_iter()
                .map(|(key, label)| LegendEntry {
                    key: key.to_string(),
                    label,
                })
                .collect(),
        })
}

/// A whole document. IDs are unique by construction; everything else
/// varies, valid geometry or not: the laws are about text, not maps.
fn document() -> impl Strategy<Value = Document> {
    (
        0usize..3,
        vec((1usize..4, 0usize..2), 1..4),
        0usize..4,
        0usize..3,
        0usize..3,
    )
        .prop_flat_map(|(level_count, groups, connections, notes, exports)| {
            // With levels, each group of rooms is one level; without, one group.
            let levels: Vec<String> = (0..level_count.min(groups.len()))
                .map(|k| format!("level_{k}"))
                .collect();
            let groups = if levels.is_empty() {
                groups[..1].to_vec()
            } else {
                groups[..levels.len()].to_vec()
            };
            let mut spaces: Vec<BoxedStrategy<Space>> = Vec::new();
            let mut all: Vec<String> = Vec::new();
            for (g, (rooms, corridors)) in groups.iter().enumerate() {
                let level = levels.get(g).cloned();
                let ids: Vec<String> = (0..*rooms).map(|k| format!("room_{g}_{k}")).collect();
                for (k, id) in ids.iter().enumerate() {
                    spaces.push(
                        room(id.clone(), level.clone(), ids[..k].to_vec(), (g + k) % 3).boxed(),
                    );
                }
                for k in 0..*corridors {
                    spaces.push(
                        corridor(format!("hall_{g}_{k}"), level.clone(), ids.clone()).boxed(),
                    );
                    all.push(format!("hall_{g}_{k}"));
                }
                all.extend(ids);
            }
            let connections: Vec<_> = (0..connections)
                .map(|k| connection(format!("link_{k}"), all.clone()))
                .collect();
            let notes: Vec<_> = (0..notes)
                .map(|k| {
                    (
                        of(select(all.clone())),
                        long_text(),
                        of(point()),
                        visibility(),
                        comments(),
                    )
                        .prop_map(
                            move |(target, text, position, visibility, comments)| Note {
                                id: format!("note_{k}"),
                                target,
                                text,
                                position,
                                visibility,
                                reveal_when: None,
                                comments,
                            },
                        )
                })
                .collect();
            let names = ["default", "players", "print"];
            let exports: Vec<_> = (0..exports)
                .map(|k| export(names[k].to_string(), levels.clone()))
                .collect();
            let level_names = vec(of(text()), levels.len()..=levels.len());
            let header = of((
                of(Just(1u32)),
                vec(
                    of(select(vec!["^1.2", ">=0.3, <2"]).prop_map(str::to_string)),
                    0..3,
                ),
            ));
            let settings = of((of(word()), of(word()), of(0i64..1000), of(positive())));
            (
                (
                    of(text()),
                    of(of(text())),
                    settings,
                    of(render()),
                    of(legend()),
                    properties(vec![]),
                ),
                level_names,
                spaces,
                connections,
                notes,
                exports,
                header,
                comments(),
            )
                .prop_map(
                    move |(
                        (name, grid, settings, render, legend, properties),
                        level_names,
                        spaces,
                        connections,
                        notes,
                        exports,
                        header,
                        comments,
                    )| {
                        let dungeon = Dungeon {
                            id: "generated".to_string(),
                            name,
                            grid: grid.map(|unit| Grid { unit, ty: None }),
                            settings: settings.map(
                                |(default_wall, default_floor, seed, max_stub)| Settings {
                                    default_wall,
                                    default_floor,
                                    seed,
                                    max_stub,
                                },
                            ),
                            render,
                            legend,
                            properties,
                            levels: levels
                                .iter()
                                .zip(level_names)
                                .map(|(id, name)| Level {
                                    id: id.clone(),
                                    name,
                                    comments: Comments::default(),
                                })
                                .collect(),
                            spaces,
                            connections,
                            elements: Vec::new(),
                            notes,
                            exports,
                            comments,
                        };
                        Document {
                            schema: None,
                            dedalo: header.map(|(version, plugins)| Header {
                                version,
                                plugins: plugins
                                    .into_iter()
                                    .enumerate()
                                    .map(|(k, version)| PluginRequirement {
                                        id: format!("plugin_{k}"),
                                        version,
                                    })
                                    .collect(),
                                comments: Comments::default(),
                            }),
                            dungeon,
                            comments: Comments::default(),
                        }
                    },
                )
        })
}

/// Text with the lenient spellings the language accepts: the laws about
/// `fmt` must hold for what people type, not only for canonical text.
fn loosen(canonical: &str, seed: u64) -> String {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut out = String::new();
    for line in canonical.lines() {
        let indent = line.len() - line.trim_start().len();
        // Indentation and blank lines are free; so are trailing spaces.
        match next() % 5 {
            0 => out.push_str(&" ".repeat(indent / 2)),
            1 => out.push('\t'),
            _ => out.push_str(&line[..indent]),
        }
        out.push_str(line.trim_start());
        if next() % 4 == 0 {
            out.push_str("  ");
        }
        out.push('\n');
        if next() % 6 == 0 && !line.trim_end().ends_with("EOT") && !canonical.contains("<<") {
            out.push('\n');
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 192, ..ProptestConfig::default() })]

    /// Law 1: `json(dsl(J)) = J` exactly.
    #[test]
    fn text_written_from_a_document_reads_back_as_that_document(doc in document()) {
        let cat = Catalog::builtin();
        let text = to_dsl(&doc, &cat);
        let back = json(&text, &cat).map_err(|e| TestCaseError::fail(format!("{e}\n{text}")))?;
        prop_assert!(back == doc, "{}\n{}", difference(&doc, &back), text);
        // JSON survives its own serialisation.
        let as_json = serde_json::to_string(&doc).unwrap();
        prop_assert_eq!(&serde_json::from_str::<Document>(&as_json).unwrap(), &doc);
        // Law 2 follows for generated text: writing again changes nothing.
        prop_assert_eq!(to_dsl(&back, &cat), text);
    }

    /// Law 3: `fmt(fmt(S)) = fmt(S)` and `json(fmt(S)) = json(S)`.
    #[test]
    fn formatting_is_idempotent_and_keeps_the_meaning(doc in document(), seed in any::<u64>()) {
        let cat = Catalog::builtin();
        let canonical = to_dsl(&doc, &cat);
        // Written text is already canonical.
        prop_assert_eq!(&fmt(&canonical, &cat), &canonical);
        let loose = loosen(&canonical, seed);
        let Ok(meaning) = json(&loose, &cat) else {
            // Loosening broke a heredoc or a string; nothing to compare.
            return Ok(());
        };
        let once = fmt(&loose, &cat);
        prop_assert_eq!(&fmt(&once, &cat), &once, "\n{}", loose);
        let after = json(&once, &cat).map_err(|e| TestCaseError::fail(format!("{e}\n{once}")))?;
        prop_assert_eq!(after, meaning, "\n{}", once);
    }
}

#[test]
fn the_generator_reaches_every_part_of_the_model() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let mut runner = TestRunner::deterministic();
    let mut seen: BTreeMap<&str, bool> = BTreeMap::new();
    for _ in 0..200 {
        let doc = document().new_tree(&mut runner).unwrap().current();
        let d = &doc.dungeon;
        *seen.entry("levels").or_default() |= !d.levels.is_empty();
        *seen.entry("one level").or_default() |= d.levels.is_empty();
        *seen.entry("corridors").or_default() |=
            d.spaces.iter().any(|s| s.kind == SpaceKind::Corridor);
        *seen.entry("elements").or_default() |= d.spaces.iter().any(|s| !s.elements.is_empty());
        *seen.entry("connections").or_default() |= !d.connections.is_empty();
        *seen.entry("notes").or_default() |= !d.notes.is_empty();
        *seen.entry("exports").or_default() |= !d.exports.is_empty();
        *seen.entry("header").or_default() |= doc.dedalo.is_some();
        *seen.entry("comments").or_default() |= d.spaces.iter().any(|s| !s.comments.is_empty());
        *seen.entry("properties").or_default() |= d.spaces.iter().any(|s| !s.properties.is_empty());
    }
    let missing: Vec<&&str> = seen
        .iter()
        .filter(|(_, hit)| !**hit)
        .map(|(k, _)| k)
        .collect();
    assert!(missing.is_empty(), "never generated: {missing:?}");
}
