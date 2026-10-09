use crate::prelude::*;

/// Resizing remaps the cursor onto the reflowed rows, so later writes land
/// after the previous text instead of inside earlier lines (or panicking
/// when the terminal shrinks).
#[test]
fn shrink_keeps_cursor_on_its_logical_line() {
    let mut app = get_test_app();

    app.add_systems(Startup, |mut commands: Commands| {
        let TestTerm { term, fg } = spawn_test_term(&mut commands, VtSize { cols: 20, rows: 6 });
        commands.write_message(write(term, fg, "top\r\n0123456789ABCDEFGHI"));
    });

    app.add_step(
        0,
        |test: Res<TestTerm>,
         q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let terminfo = r!(q_term.get(test.term).ok());
            if terminfo.lines(&q_lines).count() < 2 {
                return;
            }
            commands
                .entity(test.term)
                .insert(VtSize { cols: 5, rows: 3 });
            next.set(Step(1));
        },
    );
    app.add_step(
        1,
        |test: Res<TestTerm>,
         q_term: Query<TermInfo>,
         mut commands: Commands,
         mut next: ResMut<NextState<Step>>| {
            let terminfo = r!(q_term.get(test.term).ok());
            if terminfo.ready.is_none() || terminfo.size.cols != 5 {
                return;
            }
            commands.write_message(write(test.term, test.fg, "xyz"));
            next.set(Step(2));
        },
    );
    app.add_step(
        2,
        |test: Res<TestTerm>,
         q_term: Query<TermInfo>,
         q_lines: Query<(Entity, &VtLine)>,
         mut commands: Commands| {
            let terminfo = r!(q_term.get(test.term).ok());
            let lines = terminfo
                .lines(&q_lines)
                .map(|(_, line)| line.as_string())
                .collect::<Vec<_>>();
            if lines.len() < 3 {
                return;
            }
            // How auto-wrap splits logical lines is not under test here.
            r!(commands.assert(
                lines[0] == "top" && lines[1..].concat() == "0123456789ABCDEFGHIxyz",
                format!("new text did not follow the cursor: {lines:?}"),
            ));
            commands.write_message(AppExit::Success);
        },
    );

    assert!(app.run().is_success());
}
