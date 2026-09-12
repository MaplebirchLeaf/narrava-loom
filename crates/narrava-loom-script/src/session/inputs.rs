//! 输入提交后的呈现同步；从权威 State 读取，不重放 Passage 或执行作者回调。

use narrava_loom_core::{
    expression::{
        Expression,
        evaluator::{evaluate_with, value_to_text},
    },
    macro_runtime::parse_input_receiver,
    semantic::{SemanticInputBinding, SemanticInputKind},
};

use super::*;

impl RuntimeSession<'_, '_> {
    /// 所有同源控件在 Reaction 完成后一起同步；失败时不提交半份呈现。
    pub(super) fn sync_presented_inputs(&mut self) -> Result<bool, HostErrorDto> {
        let Some(previous) = self.presented.as_ref() else {
            return Ok(false);
        };
        let mut update: HostUpdate = previous.as_ref().clone();
        update.update_inputs(|binding: &mut SemanticInputBinding| {
            let receiver: Expression<'_> =
                parse_input_receiver(&binding.receiver).map_err(diagnostic)?;
            let current: Value = evaluate_with(&receiver, &self.state)
                .map_err(|error| input_error(format!("输入路径无法读取：{error:?}")))?;
            match &mut binding.kind {
                SemanticInputKind::Text { value } => {
                    *value = value_to_text(&current)
                        .ok_or_else(|| input_error("文字输入当前值必须能转换为文字"))?;
                }
                SemanticInputKind::Checkbox {
                    checked, selected, ..
                }
                | SemanticInputKind::Radio {
                    value: checked,
                    selected,
                    ..
                } => {
                    *selected = checked.matches_value(&current);
                }
            }
            Ok::<(), HostErrorDto>(())
        })?;
        if &update == previous.as_ref() {
            return Ok(false);
        }
        self.presented = Some(Rc::new(update));
        Ok(true)
    }
}

fn input_error(message: impl Into<String>) -> HostErrorDto {
    HostErrorDto::new("runtime_session.input_state", message)
}
