use crate::prelude::*;

/// Verifies that writing plain text "Hello, world!" to a terminal produces
/// the expected entity hierarchy:
///
/// ```text
/// Terminal (VtLineTarget, VtCursor, VtScrollPos, VtSize, VtViewport)
///   └─ VtLine { cells: [H,e,l,l,o,,,_,w,o,r,l,d,!] }
///       └─ VtRow { offset: 0 } + VtViewportRow
/// ```
#[test]
fn hello_world_hierarchy() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let term_id = commands.spawn(Terminal).id();
        commands
            .entity(term_id)
            .insert(VtSize { cols: 80, rows: 24 });
        commands.write_message(TermMsg::write(term_id, "Hello, world!"));
    });

    app.add_step(
        0,
        |q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         q_rows: Query<(Entity, &VtRow)>,
         q_rowtargets: Query<&VtRowTarget, With<VtLine>>,
         q_viewport_rows: Query<(Entity, &VtViewportRow)>,
         mut commands: Commands| {
            let terminfo = r!(q_term.single());

            // Lines may not be spawned yet (deferred commands).
            let lines: Vec<_> = terminfo.lines(&q_lines).collect();
            if lines.is_empty() {
                return;
            }

            // --- VtLine: one line containing "Hello, world!" ---
            assert_eq!(lines.len(), 1, "Expected exactly 1 line");
            let (_, line) = &lines[0];
            assert_eq!(line.as_string(), "Hello, world!");
            assert_eq!(line.cells().len(), 13);
            for cell in line.cells() {
                assert_eq!(cell.style, VtCellStyle::default());
            }

            // --- VtRow: one row at offset 0 ---
            let rows: Vec<_> = terminfo.rows(&q_rowtargets, &q_rows).collect();
            assert_eq!(rows.len(), 1, "Expected exactly 1 row");
            let (row_id, row) = &rows[0];
            assert_eq!(row.offset, 0);

            // --- VtViewport: the row is visible ---
            let viewport_rows: Vec<_> = terminfo.viewport_rows(&q_viewport_rows).collect();
            assert_eq!(viewport_rows.len(), 1, "Expected 1 viewport row");
            let (vp_row_id, _) = &viewport_rows[0];
            assert_eq!(
                vp_row_id, row_id,
                "Viewport row entity should match the data row entity"
            );

            // --- Cursor: at end of written text ---
            assert_eq!(terminfo.cursor.col, 13);
            assert_eq!(terminfo.cursor.row, 0);
            assert!(!terminfo.cursor.pending_wrap);

            // --- Scroll position: at bottom ---
            assert_eq!(terminfo.scroll_pos.0, 0);

            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
