//! 共享 Runtime 在脚本未覆盖时执行跨 Passage Widget，参数帧不能泄漏。
use super::support::{navigate, ready, text, with_runtime};
use narrava_loom_protocol::{HostUpdateDto, RuntimeCommand};

#[test]
fn runtime_widgets_share_definitions_and_keep_nested_arguments_scoped() {
    let story: &str = r#":: Start
<<card "outer">>
<<link [[继续|Next]]>><</link>>
:: Next
<<card "next">>
:: Cards [widget]
<<widget "card">>
<<print @args[0]>>
<<badge "inner">>
<<print @args[0]>>
<</widget>>
:: Badges [widget]
<<widget "badge">>
<<print @args[0]>>
<</widget>>
"#;
    with_runtime(story, "", |runtime| {
        let frame: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let output: String = text(&frame);
        assert!(output.contains("outerinnerouter"), "{output}");
        let next: HostUpdateDto = navigate(runtime, &frame, "Next").1;
        let output: String = text(&next);
        assert!(output.contains("nextinnernext"), "{output}");
    });
}

#[test]
fn script_macro_override_still_precedes_twee_widget() {
    let story: &str =
        ":: Start\n<<card>>\n:: Cards [widget]\n<<widget card>>\noriginal\n<</widget>>\n";
    let script: &str = "Macro.add('card', {handler: () => 'override'});";
    with_runtime(story, script, |runtime| {
        let frame: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        assert_eq!(text(&frame), "override");
    });
}
