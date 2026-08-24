//! Story 聚合与语义检查。

use super::*;

impl<'source> Story<'source> {
    /// 编译所有 Twee Source，并忽略其他 SourceKind。
    pub fn build(sources: &'source [Source]) -> Result<Self, StoryError<'source>> {
        let mut passages: Vec<Passage<'source>> = Vec::new();

        for source in sources {
            if source.kind != SourceKind::Twee {
                continue;
            }

            let tokens: Vec<Token<'source>> = lex(source);
            let parsed: Vec<Passage<'source>> = parse(&tokens).map_err(StoryError::Parse)?;
            passages.extend(parsed);
        }

        Self::from_passages(passages)
    }

    /// 汇总 Passage，并执行跨 Source 的名称检查。
    pub fn from_passages(passages: Vec<Passage<'source>>) -> Result<Self, StoryError<'source>> {
        validate(&passages).map_err(StoryError::Semantic)?;
        Ok(Self { passages })
    }

    /// 按区分大小写的名称查找 Passage。
    pub fn passage(&self, name: &str) -> Option<&Passage<'source>> {
        self.passages
            .iter()
            .find(|passage: &&Passage<'source>| passage.name == name)
    }
}

pub fn validate<'source>(passages: &[Passage<'source>]) -> Result<(), SemanticError<'source>> {
    let mut names: HashSet<&str> = HashSet::new();

    for passage in passages {
        let inserted: bool = names.insert(passage.name);
        if !inserted {
            return Err(SemanticError {
                source: passage.source,
                name: passage.name,
                kind: SemanticErrorKind::DuplicatePassageName,
                span: passage.span,
            });
        }
    }

    Ok(())
}
