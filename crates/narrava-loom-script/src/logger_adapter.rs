//! 脚本与 Host 共用的有界 Core Logger；日志不属于 State 事务。

use std::cell::{Ref, RefCell};

use boa_engine::{
    Context, Finalize, JsArgs, JsData, JsNativeError, JsResult, JsValue, NativeFunction, Trace,
    js_string,
};
use narrava_loom_core::{
    diagnostic::{Diagnostic, DiagnosticSeverity},
    logger::{LogEvent, LogFilter, LogLevel, LogRecord, LogSubscriptionId, Logger},
};
use narrava_loom_protocol::{HostErrorDto, HostLogLevelDto, HostLogRecordDto};
use serde::Deserialize;

use crate::{EcmaBinding, EcmaRuntime, protocol_adapter};

#[derive(Trace, Finalize, JsData)]
struct Logs {
    #[unsafe_ignore_trace]
    logger: RefCell<Logger>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Filter {
    minimum_level: Option<HostLogLevelDto>,
    target: Option<String>,
}

pub(super) fn install(context: &mut Context) -> JsResult<()> {
    context.insert_data(Logs {
        logger: RefCell::new(Logger::new()),
    });
    type NativeCall = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
    for (name, length, function) in [
        ("__narravaLoggerLog", 3, log as NativeCall),
        ("__narravaLoggerSubscribe", 1, subscribe as NativeCall),
        ("__narravaLoggerTake", 1, take as NativeCall),
        ("__narravaLoggerUnsubscribe", 1, unsubscribe as NativeCall),
    ] {
        context.register_global_builtin_callable(
            js_string!(name),
            length,
            NativeFunction::from_fn_ptr(function),
        )?;
    }
    Ok(())
}

impl EcmaBinding {
    /// 快照不会清空日志，也不会开启故事事务。
    pub fn log_records(&self) -> Vec<HostLogRecordDto> {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        logs(&runtime.context)
            .logger
            .borrow()
            .get()
            .iter()
            .map(record)
            .collect()
    }

    pub fn record_host_error(&self, error: &HostErrorDto) {
        self.record_diagnostic(&protocol_adapter::core_diagnostic(error));
    }

    /// 失败诊断与失败前的作者日志均保留，便于审核回滚原因。
    pub fn record_diagnostic(&self, diagnostic: &Diagnostic) {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        let level: LogLevel = match diagnostic.severity {
            DiagnosticSeverity::Error => LogLevel::Error,
            DiagnosticSeverity::Warning => LogLevel::Warn,
            DiagnosticSeverity::Note => LogLevel::Info,
        };
        logs(&runtime.context).logger.borrow_mut().log(
            LogEvent::new(level, "runtime", &diagnostic.message)
                .with_diagnostic(diagnostic.clone()),
        );
    }
}

fn logs(context: &Context) -> &Logs {
    context
        .get_data::<Logs>()
        .expect("Logger bridge 必须在执行脚本前安装")
}

fn log(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let level: String = string_argument(arguments, 0)?;
    let level: HostLogLevelDto = serde_json::from_value(serde_json::Value::String(level))
        .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
    let target: String = string_argument(arguments, 1)?;
    let message: String = string_argument(arguments, 2)?;
    logs(context)
        .logger
        .borrow_mut()
        .log(LogEvent::new(core_level(level), &target, &message));
    Ok(JsValue::undefined())
}

fn subscribe(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value: &JsValue = arguments.get_or_undefined(0);
    let filter: Filter = if value.is_undefined() {
        Filter::default()
    } else {
        let json: serde_json::Value = value
            .to_json(context)?
            .ok_or_else(|| JsNativeError::typ().with_message("Logger filter 必须是对象"))?;
        serde_json::from_value(json)
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?
    };
    let id: LogSubscriptionId = logs(context).logger.borrow_mut().subscribe(LogFilter {
        minimum_level: filter.minimum_level.map(core_level),
        target: filter.target,
    });
    Ok(JsValue::new(id.get() as f64))
}

fn take(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id: LogSubscriptionId = subscription(arguments)?;
    let records: Option<Vec<LogRecord>> = logs(context).logger.borrow_mut().take(id);
    let Some(records) = records else {
        return Ok(JsValue::undefined());
    };
    let records: Vec<HostLogRecordDto> = records.iter().map(record).collect();
    let json: serde_json::Value = serde_json::to_value(records)
        .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
    JsValue::from_json(&json, context)
}

fn unsubscribe(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let id: LogSubscriptionId = subscription(arguments)?;
    Ok(JsValue::new(
        logs(context).logger.borrow_mut().unsubscribe(id),
    ))
}

fn subscription(arguments: &[JsValue]) -> JsResult<LogSubscriptionId> {
    let value: f64 = arguments
        .get_or_undefined(0)
        .as_number()
        .filter(|value| {
            value.is_finite()
                && value.fract() == 0.0
                && (0.0..=9_007_199_254_740_991.0).contains(value)
        })
        .ok_or_else(|| {
            JsNativeError::typ().with_message("Logger subscription 必须是非负安全整数")
        })?;
    Ok(LogSubscriptionId::from_value(value as u64))
}

fn string_argument(arguments: &[JsValue], index: usize) -> JsResult<String> {
    arguments
        .get_or_undefined(index)
        .as_string()
        .map(|value| value.to_std_string_escaped())
        .ok_or_else(|| {
            JsNativeError::typ()
                .with_message("Logger 的级别、target 和 message 必须是字符串")
                .into()
        })
}

fn core_level(level: HostLogLevelDto) -> LogLevel {
    match level {
        HostLogLevelDto::Trace => LogLevel::Trace,
        HostLogLevelDto::Debug => LogLevel::Debug,
        HostLogLevelDto::Info => LogLevel::Info,
        HostLogLevelDto::Warn => LogLevel::Warn,
        HostLogLevelDto::Error => LogLevel::Error,
    }
}

fn record(record: &LogRecord) -> HostLogRecordDto {
    HostLogRecordDto {
        sequence: record.sequence.get(),
        level: match record.event.level {
            LogLevel::Trace => HostLogLevelDto::Trace,
            LogLevel::Debug => HostLogLevelDto::Debug,
            LogLevel::Info => HostLogLevelDto::Info,
            LogLevel::Warn => HostLogLevelDto::Warn,
            LogLevel::Error => HostLogLevelDto::Error,
        },
        target: record.event.target.clone(),
        message: record.event.message.clone(),
        diagnostic: record
            .event
            .diagnostic
            .clone()
            .map(protocol_adapter::diagnostic),
    }
}
