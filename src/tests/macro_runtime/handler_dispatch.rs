use super::*;

#[test]
fn exposes_explicit_inline_handler_input() {
    let arguments: Vec<Value> = vec![Value::Number(2.0)];
    let mut context: usize = 4;
    let invocation: MacroInvocation<'_, (), usize> =
        MacroInvocation::inline("double", "2", &arguments, &mut context);

    assert_eq!(invocation.name, "double");
    assert_eq!(invocation.raw_arguments, "2");
    assert_eq!(invocation.arguments, arguments.as_slice());
    assert_eq!(invocation.body, MacroInvocationBody::Inline);
    *invocation.context += 1;
    assert_eq!(context, 5);
}

#[test]
fn keeps_container_body_and_async_handle_explicit() {
    let body: Vec<&str> = vec!["first", "second"];
    let arguments: Vec<Value> = Vec::new();
    let mut context: () = ();
    let invocation: MacroInvocation<'_, &str, ()> =
        MacroInvocation::container("delay", "", &arguments, &body, &mut context);
    let outcome: MacroHandlerOutcome<Value, u64> = MacroHandlerOutcome::Pending(17);

    assert_eq!(
        invocation.body,
        MacroInvocationBody::Container(body.as_slice())
    );
    assert_eq!(outcome, MacroHandlerOutcome::Pending(17));
}

#[test]
fn rejects_body_mismatch_before_calling_handler() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "handler",
    );
    let arguments: Vec<Value> = Vec::new();
    let body: Vec<()> = vec![()];
    let mut context: () = ();
    let invocation: MacroInvocation<'_, (), ()> =
        MacroInvocation::container("sample", "", &arguments, &body, &mut context);
    let mut called: bool = false;

    let result: Result<MacroHandlerOutcome<Value, u64>, MacroDispatchError<&str, u64>> =
        dispatch_macro(&definition, invocation, |_, _| {
            called = true;
            Ok(MacroHandlerOutcome::Complete(Value::Undefined))
        });

    assert_eq!(
        result,
        Err(MacroDispatchError::BodyKindMismatch {
            expected: MacroBodyKind::Inline,
            actual: MacroBodyKind::Container,
        })
    );
    assert!(!called);
}

#[test]
fn rejects_pending_result_from_sync_handler_without_losing_handle() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Sync,
        "handler",
    );
    let arguments: Vec<Value> = Vec::new();
    let mut context: () = ();
    let invocation: MacroInvocation<'_, (), ()> =
        MacroInvocation::inline("sample", "", &arguments, &mut context);

    let result: Result<MacroHandlerOutcome<Value, u64>, MacroDispatchError<&str, u64>> =
        dispatch_macro(&definition, invocation, |_, _| {
            Ok(MacroHandlerOutcome::Pending(17))
        });

    assert_eq!(result, Err(MacroDispatchError::UnexpectedPending(17)));
}

#[test]
fn allows_async_handler_to_complete_now_or_return_pending() {
    let definition: MacroDefinition<&str> = MacroDefinition::new(
        MacroBodyKind::Inline,
        MacroArgumentKind::Raw,
        MacroExecutionKind::Async,
        "handler",
    );
    let arguments: Vec<Value> = Vec::new();
    let mut first_context: () = ();
    let first: MacroInvocation<'_, (), ()> =
        MacroInvocation::inline("sample", "", &arguments, &mut first_context);
    let complete: Result<MacroHandlerOutcome<Value, u64>, MacroDispatchError<&str, u64>> =
        dispatch_macro(&definition, first, |_, _| {
            Ok(MacroHandlerOutcome::Complete(Value::Undefined))
        });
    let mut second_context: () = ();
    let second: MacroInvocation<'_, (), ()> =
        MacroInvocation::inline("sample", "", &arguments, &mut second_context);
    let pending: Result<MacroHandlerOutcome<Value, u64>, MacroDispatchError<&str, u64>> =
        dispatch_macro(&definition, second, |_, _| {
            Ok(MacroHandlerOutcome::Pending(23))
        });

    assert_eq!(
        complete,
        Ok(MacroHandlerOutcome::Complete(Value::Undefined))
    );
    assert_eq!(pending, Ok(MacroHandlerOutcome::Pending(23)));
}
