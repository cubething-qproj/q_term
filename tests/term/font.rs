//! The grid's text takes the [`VtUi`]'s [`TextFont`] and [`LineHeight`].
//! Bevy text doesn't inherit them, so without this the glyphs render at the
//! default size while q_term sizes the grid for the configured one.

use bevy::{ecs::system::RunSystemOnce, text::LineHeight};

use crate::prelude::*;

#[test]
fn rendered_text_uses_the_vtui_font() {
    let mut app = get_test_app();
    let font = TextFont::default().with_font_size(FontSize::Px(11.0));
    let line_height = LineHeight::Px(13.0);
    let term_id = app.world_mut().spawn(Terminal).id();
    let vtui_id = app
        .world_mut()
        .spawn((VtUi::new(term_id), font.clone(), line_height))
        .id();
    app.world_mut().flush();
    app.world_mut()
        .entity_mut(term_id)
        .insert(VtSize { cols: 4, rows: 2 });

    app.world_mut()
        .write_message(TermRedrawRequestedMsg { term: term_id });
    app.world_mut()
        .run_system_once(refresh_ui)
        .expect("refresh_ui ran");
    app.world_mut().flush();

    let grid_id = app
        .world()
        .get::<VtUiGridTarget>(vtui_id)
        .expect("VtUi has a grid")
        .target();
    let spans = app
        .world()
        .get::<Children>(grid_id)
        .expect("refresh_ui spawned spans");
    assert!(!spans.is_empty());
    for entity in std::iter::once(grid_id).chain(spans.iter()) {
        assert_eq!(app.world().get::<TextFont>(entity), Some(&font));
        assert_eq!(app.world().get::<LineHeight>(entity), Some(&line_height));
    }
}
