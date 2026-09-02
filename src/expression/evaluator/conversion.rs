//! Web 标量转换、整数转换与字符串字面量解码。

use super::EvalError;
use crate::expression::{
    Span,
    value::{TextValue, Value},
};

/// 转换无需调用对象原型方法的 Web 标量值。
pub(super) fn to_number(value: &Value) -> Option<f64> {
    match value {
        Value::Undefined => Some(f64::NAN),
        Value::Null => Some(0.0),
        Value::Boolean(value) => Some(if *value { 1.0 } else { 0.0 }),
        Value::Number(value) => Some(*value),
        Value::String(value) => Some(string_to_number(value)),
        Value::Array(_)
        | Value::Callable(_)
        | Value::ScriptCallable(_)
        | Value::Namespace(_)
        | Value::Object(_) => None,
    }
}

/// 字符串拼接只转换标量，拒绝集合的隐式字符串化。
pub(super) fn to_string(value: &Value) -> Option<TextValue> {
    match value {
        Value::Undefined => Some(TextValue::from("undefined")),
        Value::Null => Some(TextValue::from("null")),
        Value::Boolean(value) => Some(TextValue::from(if *value { "true" } else { "false" })),
        Value::Number(value) if value.is_nan() => Some(TextValue::from("NaN")),
        Value::Number(value) if *value == f64::INFINITY => Some(TextValue::from("Infinity")),
        Value::Number(value) if *value == f64::NEG_INFINITY => Some(TextValue::from("-Infinity")),
        Value::Number(value) if *value == 0.0 => Some(TextValue::from("0")),
        Value::Number(value) => Some(TextValue::from(value.to_string())),
        Value::String(value) => Some(value.clone()),
        Value::Array(_)
        | Value::Callable(_)
        | Value::ScriptCallable(_)
        | Value::Namespace(_)
        | Value::Object(_) => None,
    }
}

/// 字符串数值转换覆盖十进制以及 Web 常用的 0x、0o、0b 前缀。
pub(super) fn string_to_number(value: &TextValue) -> f64 {
    let Some(unicode) = value.to_unicode_string() else {
        return f64::NAN;
    };
    let value: &str = unicode.trim();
    if value.is_empty() {
        return 0.0;
    }

    let radix_value: Option<(u32, &str)> = if let Some(digits) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some((16, digits))
    } else if let Some(digits) = value
        .strip_prefix("0o")
        .or_else(|| value.strip_prefix("0O"))
    {
        Some((8, digits))
    } else {
        value
            .strip_prefix("0b")
            .or_else(|| value.strip_prefix("0B"))
            .map(|digits| (2, digits))
    };

    if let Some((radix, digits)) = radix_value {
        if digits.is_empty() {
            return f64::NAN;
        }
        return u64::from_str_radix(digits, radix)
            .map(|number| number as f64)
            .unwrap_or(f64::NAN);
    }

    value.parse::<f64>().unwrap_or(f64::NAN)
}

/// Web 按位运算先把 Number 截断并折返到有符号 32 位整数。
pub(super) fn to_int32(value: f64) -> i32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }

    let unsigned: f64 = value.trunc().rem_euclid(4_294_967_296.0);
    if unsigned >= 2_147_483_648.0 {
        (unsigned - 4_294_967_296.0) as i32
    } else {
        unsigned as i32
    }
}

/// 无符号右移和移位数量使用同一套 32 位无符号折返规则。
pub(super) fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }

    value.trunc().rem_euclid(4_294_967_296.0) as u32
}

/// 解码 Expression 支持的 JS 风格单字符转义。
///
/// `source` 不含外层引号，因此错误位置需要补回起始引号所占的一字节。
pub(super) fn decode_string(source: &str, expression_span: Span) -> Result<String, EvalError> {
    let bytes: &[u8] = source.as_bytes();
    let mut decoded: String = String::with_capacity(source.len());
    let mut index: usize = 0;

    while index < bytes.len() {
        if bytes[index] != b'\\' {
            // 非 ASCII 内容按完整 Unicode 标量复制，不能按字节截断。
            let character: char = source[index..]
                .chars()
                .next()
                .expect("有效字符串切片必须包含字符");
            decoded.push(character);
            index += character.len_utf8();
            continue;
        }

        let escape_start: usize = index;
        index += 1;
        let escaped: u8 = bytes[index];
        if escaped == b'x' {
            let Some(code_point) = read_hex(bytes, index + 1, 2) else {
                return Err(invalid_escape_span(
                    expression_span,
                    source.len(),
                    escape_start,
                    4,
                ));
            };
            let character: char = char::from_u32(code_point).expect("两位十六进制必定是有效字符");
            decoded.push(character);
            index = escape_start + 4;
            continue;
        }
        if escaped == b'u' {
            let Some(first_unit) = read_hex(bytes, index + 1, 4) else {
                return Err(invalid_escape_span(
                    expression_span,
                    source.len(),
                    escape_start,
                    6,
                ));
            };

            if (0xD800..=0xDBFF).contains(&first_unit) {
                let second_start: usize = escape_start + 6;
                let second_unit: Option<u32> = if bytes.get(second_start) == Some(&b'\\')
                    && bytes.get(second_start + 1) == Some(&b'u')
                {
                    read_hex(bytes, second_start + 2, 4)
                } else {
                    None
                };
                let Some(second_unit) = second_unit.filter(|unit| (0xDC00..=0xDFFF).contains(unit))
                else {
                    return Err(invalid_escape_span(
                        expression_span,
                        source.len(),
                        escape_start,
                        6,
                    ));
                };

                let code_point: u32 =
                    0x10000 + ((first_unit - 0xD800) << 10) + (second_unit - 0xDC00);
                let character: char =
                    char::from_u32(code_point).expect("合法代理对必须形成有效字符");
                decoded.push(character);
                index = second_start + 6;
                continue;
            }
            if (0xDC00..=0xDFFF).contains(&first_unit) {
                return Err(invalid_escape_span(
                    expression_span,
                    source.len(),
                    escape_start,
                    6,
                ));
            }

            let character: char = char::from_u32(first_unit).expect("非代理项 u16 必定是有效字符");
            decoded.push(character);
            index = escape_start + 6;
            continue;
        }

        let character: char = match escaped {
            b'\\' => '\\',
            b'\'' => '\'',
            b'"' => '"',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'b' => '\u{0008}',
            b'f' => '\u{000c}',
            b'v' => '\u{000b}',
            b'0' => '\0',
            _ => {
                let start: usize = expression_span.start + 1 + escape_start;
                return Err(EvalError::InvalidStringEscape(Span {
                    start,
                    end: start + 2,
                }));
            }
        };

        decoded.push(character);
        index += 1;
    }

    Ok(decoded)
}

/// 读取固定数量的 ASCII 十六进制数字，不接受缺位或其它字符。
fn read_hex(bytes: &[u8], start: usize, digits: usize) -> Option<u32> {
    let end: usize = start.checked_add(digits)?;
    let source: &[u8] = bytes.get(start..end)?;
    let mut value: u32 = 0;

    for byte in source {
        let digit: u32 = char::from(*byte).to_digit(16)?;
        value = (value << 4) | digit;
    }

    Some(value)
}

/// 错误范围覆盖完整转义；输入提前结束时则收束到字符串内容末尾。
fn invalid_escape_span(
    expression_span: Span,
    source_length: usize,
    escape_start: usize,
    width: usize,
) -> EvalError {
    let content_start: usize = expression_span.start + 1;
    let start: usize = content_start + escape_start;
    let end: usize = (start + width).min(content_start + source_length);
    EvalError::InvalidStringEscape(Span { start, end })
}
