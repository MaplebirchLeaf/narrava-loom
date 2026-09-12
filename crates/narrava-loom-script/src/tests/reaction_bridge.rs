use super::support::{ready, text, with_runtime};
use narrava_loom_protocol::RuntimeCommand;

#[test]
fn reaction_queries_work_inside_lifecycle_event_and_state_conditions() {
    for trigger in ["lifecycle:true", "event:'probe:run'", "state:'$score'"] {
        let script: String = format!(
            r#"
Reaction.add({{
  id:'probe', {trigger},
  cond:() => Reaction.get('probe').enabled && Reaction.get('missing') === undefined,
  widget:'queried'
}});
Event.emit('probe:run');
"#
        );
        with_runtime(":: Start\n<<set $score to 1>>body", &script, |runtime| {
            let (_, frame) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
            assert!(text(&frame).contains("queried"), "{trigger}");
        });
    }
}

#[test]
fn reaction_conditions_reject_mutations_as_catchable_script_errors() {
    for mutation in [
        "Reaction.add({id:'new',event:'probe:new',widget:'new'})",
        "Reaction.enable('probe')",
        "Reaction.disable('probe')",
        "Reaction.reset('probe')",
    ] {
        let script: String = format!(
            r#"
Reaction.add({{
  id:'probe', lifecycle:true,
  cond:() => {{
    try {{ {mutation}; return false; }}
    catch (error) {{ State.variables.set('error', String(error)); return true; }}
  }},
  widget:'condition completed'
}});
Macro.add('mutateAfter', {{handler:() => String(Reaction.disable('probe'))}});
"#
        );
        with_runtime(
            ":: Start\n<<print $error>> <<mutateAfter>>",
            &script,
            |runtime| {
                let (_, frame) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
                let output: String = text(&frame);
                assert!(
                    output.contains("reaction.resolving"),
                    "{mutation}: {output}"
                );
                assert!(output.contains("condition completed"));
                assert!(output.contains("true"));
            },
        );
    }
}

#[test]
fn reaction_emit_callbacks_read_the_start_of_resolution_view() {
    let script: &str = r#"
Reaction.add({id:'first',lifecycle:true,once:true,widget:'first'});
Reaction.add({
  id:'second',lifecycle:true,
  cond:() => Reaction.get('first').triggered === 0,
  emit:{name:'probe:checked',payload:() => ({first:Reaction.get('first').triggered})}
});
Reaction.add({id:'third',event:'probe:checked',cond:data => data.first === 0,widget:'snapshot read'});
Macro.add('readAfter', {handler:() => String(Reaction.get('first'))});
"#;
    with_runtime(":: Start\n<<readAfter>>", script, |runtime| {
        let (_, frame) = ready(runtime.execute(RuntimeCommand::Start).unwrap());
        let output: String = text(&frame);
        assert!(output.contains("snapshot read"), "{output}");
        assert!(output.contains("undefined"));
    });
}

#[test]
fn reaction_query_view_is_released_after_a_condition_error() {
    let script: &str = r#"
State.global.set('firstAttempt', true);
Reaction.add({
  id:'probe',lifecycle:true,
  cond:() => {
    if (State.global.get('firstAttempt')) throw new Error('condition failed');
    return Reaction.get('probe').enabled;
  },
  widget:'recovered'
});
"#;
    with_runtime(":: Start\nbody", script, |runtime| {
        let error = runtime.execute(RuntimeCommand::Start).unwrap_err();
        assert!(error.message.contains("condition"));
        // A second resolution must fail in the same controlled way, without a stale guard or panic.
        let retry = runtime.execute(RuntimeCommand::Start).unwrap_err();
        assert_eq!(retry.code, error.code);
        assert_eq!(retry.message, error.message);
    });
}
