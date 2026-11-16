//! PNG and PDF, both derived from the SVG the scene serialises to (spec 9.6).
//!
//! The boundary is an SVG string in and bytes out, so the browser can load
//! this code as a separate module on the first raster or PDF export.

use std::sync::Arc;

use dedalo_model::{Orientation, Page};
use resvg::{tiny_skia, usvg};

/// Longest side of a PNG, in pixels.
pub const MAX_PNG_SIDE: u32 = 16_384;

fn options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    // Only the bundled fonts: live text renders the same on every system.
    let mut db = usvg::fontdb::Database::new();
    db.load_font_data(dedalo_scene::font::REGULAR.to_vec());
    db.load_font_data(dedalo_scene::font::BOLD.to_vec());
    db.set_sans_serif_family(dedalo_scene::font::FAMILY);
    opt.fontdb = Arc::new(db);
    opt.font_family = dedalo_scene::font::FAMILY.to_string();
    opt
}

fn tree(svg: &str) -> Result<usvg::Tree, String> {
    usvg::Tree::from_str(svg, &options()).map_err(|e| format!("the SVG could not be read: {e}"))
}

/// The pixel size an SVG declares, before it is drawn.
pub fn svg_size(svg: &str) -> Result<(f32, f32), String> {
    let size = tree(svg)?.size();
    Ok((size.width(), size.height()))
}

/// Rasterises an SVG at the pixel size it declares.
pub fn png(svg: &str) -> Result<Vec<u8>, String> {
    let tree = tree(svg)?;
    let (w, h) = (
        tree.size().width().ceil() as u32,
        tree.size().height().ceil() as u32,
    );
    if w > MAX_PNG_SIDE || h > MAX_PNG_SIDE {
        return Err(format!(
            "the image would be {w} by {h} pixels; the largest side is {MAX_PNG_SIDE}"
        ));
    }
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("the image has no area")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {e}"))
}

/// Page sizes in points.
fn page_size(page: Page) -> Option<(f32, f32)> {
    match page {
        Page::Fit => None,
        Page::A4 => Some((595.276, 841.89)),
        Page::A3 => Some((841.89, 1190.551)),
        Page::Letter => Some((612.0, 792.0)),
    }
}

/// Converts an SVG to a vector PDF.
///
/// With `tabletop` (the SVG's pixels per cell), one cell prints one inch,
/// tiled across as many pages as it takes, each with a light cut line
/// around its printable area; the page defaults to A4. Otherwise one page:
/// with `fit` the page is the drawing at 96 pixels per inch, else the
/// drawing is scaled to fit the page inside a 36-point margin, centred.
pub fn pdf(
    svg: &str,
    page: Page,
    orientation: Option<Orientation>,
    tabletop: Option<u32>,
) -> Result<Vec<u8>, String> {
    use krilla::geom::{PathBuilder, Size, Transform};
    use krilla::page::PageSettings;
    use krilla_svg::{SurfaceExt, SvgSettings};

    let tree = tree(svg)?;
    let settings = krilla::SerializeSettings {
        xmp_metadata: false,
        enable_tagging: false,
        ..Default::default()
    };
    let mut document = krilla::Document::new_with(settings);
    let oriented = |(a, b): (f32, f32), wide: bool| match orientation {
        Some(Orientation::Landscape) => (b, a),
        Some(Orientation::Portrait) => (a, b),
        None if wide => (b, a),
        None => (a, b),
    };
    if let Some(cell_px) = tabletop {
        const MARGIN: f32 = 36.0;
        let k = 72.0 / cell_px.max(1) as f32;
        let (dw, dh) = (tree.size().width() * k, tree.size().height() * k);
        let (pw, ph) = oriented(page_size(page).unwrap_or((595.276, 841.89)), false);
        let (tw, th) = (pw - 2.0 * MARGIN, ph - 2.0 * MARGIN);
        let (cols, rows) = (
            (dw / tw).ceil().max(1.0) as usize,
            (dh / th).ceil().max(1.0) as usize,
        );
        if cols * rows > 400 {
            return Err(format!(
                "a tabletop print would take {} pages; the limit is 400",
                cols * rows
            ));
        }
        let size = Size::from_wh(dw, dh).ok_or("the drawing has no area")?;
        for row in 0..rows {
            for col in 0..cols {
                let mut pdf_page = document
                    .start_page_with(PageSettings::from_wh(pw, ph).ok_or("the page has no area")?);
                let mut surface = pdf_page.surface();
                let mut clip = PathBuilder::new();
                clip.push_rect(
                    krilla::geom::Rect::from_xywh(MARGIN, MARGIN, tw, th).ok_or("no tile")?,
                );
                let clip = clip.finish().ok_or("no tile")?;
                surface.push_clip_path(&clip, &krilla::paint::FillRule::NonZero);
                surface.push_transform(&Transform::from_translate(
                    MARGIN - col as f32 * tw,
                    MARGIN - row as f32 * th,
                ));
                surface
                    .draw_svg(
                        &tree,
                        size,
                        SvgSettings {
                            embed_text: true,
                            ..Default::default()
                        },
                    )
                    .ok_or("the SVG could not be drawn")?;
                surface.pop();
                surface.pop();
                // A light cut line around the printable area.
                surface.set_fill(None);
                surface.set_stroke(Some(krilla::paint::Stroke {
                    paint: krilla::color::rgb::Color::new(170, 170, 170).into(),
                    width: 0.5,
                    ..Default::default()
                }));
                surface.draw_path(&clip);
                surface.finish();
                pdf_page.finish();
            }
        }
        return document
            .finish()
            .map_err(|e| format!("PDF writing failed: {e:?}"));
    }
    let (w, h) = (tree.size().width() * 0.75, tree.size().height() * 0.75);
    let (pw, ph) = match page_size(page) {
        None => (w, h),
        Some(p) => oriented(p, w > h),
    };
    let margin = if page == Page::Fit { 0.0 } else { 36.0 };
    let k = ((pw - 2.0 * margin) / w)
        .min((ph - 2.0 * margin) / h)
        .min(if page == Page::Fit { 1.0 } else { f32::MAX });
    let (dw, dh) = (w * k, h * k);
    let settings = krilla::SerializeSettings {
        xmp_metadata: false,
        enable_tagging: false,
        ..Default::default()
    };
    let mut document = krilla::Document::new_with(settings);
    {
        let mut pdf_page =
            document.start_page_with(PageSettings::from_wh(pw, ph).ok_or("the page has no area")?);
        let mut surface = pdf_page.surface();
        surface.push_transform(&Transform::from_translate((pw - dw) / 2.0, (ph - dh) / 2.0));
        let size = Size::from_wh(dw, dh).ok_or("the drawing has no area")?;
        // Text is already outlines unless `text = live`; then it stays selectable.
        surface
            .draw_svg(
                &tree,
                size,
                SvgSettings {
                    embed_text: true,
                    ..Default::default()
                },
            )
            .ok_or("the SVG could not be drawn")?;
        surface.pop();
        surface.finish();
        pdf_page.finish();
    }
    document
        .finish()
        .map_err(|e| format!("PDF writing failed: {e:?}"))
}
