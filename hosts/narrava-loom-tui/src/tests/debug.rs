//! 检查器是 Host 本地覆盖层，不能借查看状态激活剧情操作。

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use narrava_loom_protocol::{
    DiagnosticLocationDto, DiagnosticSeverityDto, HostDebugSnapshotDto, HostErrorDto,
    HostLogLevelDto, HostLogRecordDto,
};

use crate::{
    TuiCommand, TuiDialogPage, TuiFrame, TuiInteraction, TuiOperation, screen::ScreenState,
};

fn snapshot() -> HostDebugSnapshotDto {
    HostDebugSnapshotDto {
        evaluation: None,
        current: Some(String::from("Start")),
        state: serde_json::json!({"variables":{"coins":3}}),
        location: serde_json::json!({"current":null}),
        truncated: true,
        logs: vec![HostLogRecordDto {
            sequence: 7,
            level: HostLogLevelDto::Error,
            target: String::from("runtime"),
            message: String::from("author failed"),
            diagnostic: Some(HostErrorDto {
                code: String::from("script.call"),
                message: String::from("author failed"),
                severity: Some(DiagnosticSeverityDto::Error),
                location: Some(Box::new(DiagnosticLocationDto {
                    source: String::from("scripts/quest.ts"),
                    start: None,
                    end: None,
                    line: Some(4),
                    column: Some(9),
                    generated: true,
                })),
            }),
        }],
    }
}

#[test]
fn inspect_command_is_available_without_an_interaction() {
    for command in ["inspect", ":inspect"] {
        let parsed: TuiCommand = TuiCommand::parse(command).unwrap();
        assert_eq!(
            parsed.resolve(&TuiFrame::default()).unwrap(),
            TuiOperation::Inspect
        );
    }
}

#[test]
fn inspection_text_keeps_source_coordinates_and_truncation_visible() {
    let lines: Vec<String> = crate::debug::snapshot_lines(&snapshot());
    let output: String = lines.join("\n");
    assert!(output.contains("coins"));
    assert!(output.contains("Location"));
    assert!(!output.contains("Random"));
    assert!(output.contains("截断"));
    assert!(
        output.contains("scripts/quest.ts:4:9 (generated)"),
        "{output}"
    );
    assert!(output.contains("script.call: author failed"));
}

#[test]
fn closing_inspection_preserves_dialog_focus_and_original_actions() {
    let frame: TuiFrame = TuiFrame {
        current: String::from("Start"),
        dialog_key: Some(String::from("story-dialog")),
        dialog_pages: vec![TuiDialogPage {
            title: String::from("Story"),
            group: String::from("弹窗 · Story"),
            lines: vec![String::from("Visible story")],
        }],
        interactions: ["first", "second"]
            .into_iter()
            .map(|id| TuiInteraction {
                group: String::from("弹窗 · Story"),
                id: Some(id.to_owned()),
                label: id.to_owned(),
                kind: "button",
                input: None,
            })
            .collect(),
        ..TuiFrame::default()
    };
    let original: TuiFrame = frame.clone();
    let mut screen: ScreenState = ScreenState::default();
    screen.move_vertical(&frame, 1);
    assert_eq!(screen.focus(), Some(1));
    screen.open_inspection(&snapshot());
    for key in [
        KeyCode::Enter,
        KeyCode::Char('b'),
        KeyCode::Down,
        KeyCode::PageDown,
        KeyCode::F(2),
    ] {
        assert!(screen.handle_inspection_key(KeyEvent::new(key, KeyModifiers::NONE)));
        assert_eq!(screen.focus(), Some(1));
    }
    assert!(screen.handle_inspection_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(!screen.handle_inspection_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
    assert_eq!(frame, original);
    screen.move_vertical(&frame, -1);
    assert_eq!(screen.focus(), Some(0), "关闭检查器后仍在原剧情弹窗内");
    assert_eq!(
        TuiCommand::Select(1).resolve(&frame).unwrap(),
        TuiOperation::Activate {
            id: String::from("second")
        }
    );
}
