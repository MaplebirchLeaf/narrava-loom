//! Script 发起的 Audio、Save 与语言请求；平台 IO 留给 Session 与 Host。

use crate::{EcmaBinding, ScriptError, js_string, script_error, value_to_json};
use boa_engine::Source;
use narrava_loom_core::expression::value::Value;

impl EcmaBinding {
    /// 原生 Twee 与 Script Audio 共用同一参数校验和有序 effect 队列。
    pub(crate) fn request_audio(&self, arguments: &[Value]) -> Result<(), ScriptError> {
        if !(1..=3).contains(&arguments.len())
            || arguments
                .iter()
                .any(|value| !matches!(value, Value::String(_)))
        {
            return Err(ScriptError::new(
                "audio.arguments",
                "audio 需要资源、可选 channel、可选 tag 字符串",
            ));
        }
        let arguments: Vec<serde_json::Value> = arguments
            .iter()
            .map(value_to_json)
            .collect::<Result<_, _>>()
            .map_err(|_| ScriptError::new("audio.arguments", "Audio 参数无效"))?;
        self.audio_control("audioMacro", serde_json::Value::Array(arguments))
    }

    pub(crate) fn audio_control(
        &self,
        method: &str,
        arguments: serde_json::Value,
    ) -> Result<(), ScriptError> {
        let source: String = format!("__narrava.{method}(...{arguments})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(source.as_bytes()))
            .map_err(|error| script_error("audio.control", error))?;
        Ok(())
    }

    pub(crate) fn take_audio(
        &self,
    ) -> Result<Vec<narrava_loom_protocol::AudioEffect>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeAudio())"))
            .map_err(|error| script_error("audio.effects", error))?;
        let json: String = js_string(&value, &mut runtime.context)?;
        serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("audio.effects", error.to_string()))
    }

    /// 取出脚本登记的 Save 请求（operation, target）；无请求时返回 `None`。
    pub fn take_save(
        &self,
    ) -> Result<Option<(narrava_loom_protocol::SaveOperation, String)>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes("JSON.stringify(__narrava.takeSave())"))
            .map_err(|error| script_error("script.save_request", error))?;
        if value.is_undefined() {
            return Ok(None);
        }
        let json = js_string(&value, &mut runtime.context)?;
        let request: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("script.save_request", error.to_string()))?;
        if request.is_null() {
            return Ok(None);
        }
        let operation = request["operation"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save operation 无效"))?;
        let target = request["target"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.save_request", "Save target 无效"))?;
        let operation: narrava_loom_protocol::SaveOperation = match operation {
            "export" => narrava_loom_protocol::SaveOperation::Export,
            "import" => narrava_loom_protocol::SaveOperation::Import,
            _ => {
                return Err(ScriptError::new(
                    "runtime_session.save_operation",
                    format!("未知 Script Save 操作：{operation}"),
                ));
            }
        };
        Ok(Some((operation, target.to_owned())))
    }

    /// 取出脚本登记的语言切换请求；无请求时返回 `None`。
    pub fn take_language(&self) -> Result<Option<String>, ScriptError> {
        let mut runtime = self.runtime.borrow_mut();
        let value = runtime
            .context
            .eval(Source::from_bytes(
                "JSON.stringify(__narrava.takeLanguage())",
            ))
            .map_err(|error| script_error("script.language_request", error))?;
        if value.is_undefined() {
            return Ok(None);
        }
        let json = js_string(&value, &mut runtime.context)?;
        let request: serde_json::Value = serde_json::from_str(&json)
            .map_err(|error| ScriptError::new("script.language_request", error.to_string()))?;
        if request.is_null() {
            return Ok(None);
        }
        let locale = request["locale"]
            .as_str()
            .ok_or_else(|| ScriptError::new("script.language_request", "I18n locale 无效"))?;
        Ok(Some(locale.to_owned()))
    }

    /// 把存档结果回传给脚本的 `Save.after` 钩子。
    pub fn complete_save(
        &self,
        operation: &str,
        target: &str,
        outcome: Result<(), &str>,
    ) -> Result<(), ScriptError> {
        let completion = match outcome {
            Ok(()) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": true })
            }
            Err(error) => {
                serde_json::json!({ "operation": operation, "target": target, "succeeded": false, "error": error })
            }
        };
        let expression = format!("__narrava.completeSave({completion})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map(|_| ())
            .map_err(|error| script_error("script.save_after", error))
    }

    /// 同步 Host 已确认的运行语言，使脚本侧 `I18n.locale` 与实际渲染语言一致。
    pub fn select_locale(&self, locale: &str) -> Result<(), ScriptError> {
        let configuration = serde_json::json!({ "locale": locale });
        let expression: String = format!("__narrava.configure({configuration})");
        self.runtime
            .borrow_mut()
            .context
            .eval(Source::from_bytes(expression.as_bytes()))
            .map_err(|error| script_error("script.i18n_locale", error))?;
        Ok(())
    }
}
