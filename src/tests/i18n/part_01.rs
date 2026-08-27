// i18n.rs 测试分片 01：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn i18n_catalog_collects_visible_text_with_structural_identity() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                text_node("森林入口", 4),
                HirBodyNode {
                    kind: HirBodyKind::Silently(vec![text_node("不可见说明", 8)]),
                    span: span(7),
                },
            ],
        }],
    };

    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);

    assert_eq!(catalog.messages().len(), 1);
    let message = &catalog.messages()[0];
    assert_eq!(message.id().as_str(), "p5:Start:body.0");
    assert_eq!(message.source(), "story/main.twee");
    assert_eq!(message.passage(), "Start");
    assert_eq!(message.text(), "森林入口");
    assert_eq!(message.span(), span(4));
}

#[test]
fn i18n_catalog_does_not_export_whitespace_only_output() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![text_node("\n  \n", 1)],
        }],
    };

    assert!(I18nCatalog::from_hir(&story).messages().is_empty());
}

#[test]
fn hard_break_splits_translatable_messages_without_exporting_markup() {
    let source: Source = Source {
        path: crate::source::SourcePath::from_path(Path::new("story/main.twee")).unwrap(),
        kind: crate::source::SourceKind::Twee,
        content: String::from(":: Start\na<br>b"),
    };
    let sources: [Source; 1] = [source];
    let twee: crate::twee::Story<'_> = crate::twee::Story::build(&sources).unwrap();
    let hir: HirStory<'_> = HirStory::lower(&twee).unwrap();
    let catalog: I18nCatalog = I18nCatalog::from_hir(&hir);
    let nmsg: String = catalog.template("en").to_nmsg();

    assert_eq!(catalog.messages().len(), 2);
    assert_eq!(catalog.messages()[0].text(), "a");
    assert_eq!(catalog.messages()[1].text(), "b");
    assert!(!nmsg.contains("<br>"));
    assert!(!nmsg.contains("{br}"));
    assert!(matches!(hir.passages[0].body[1].kind, HirBodyKind::HardBreak));
}

#[test]
fn hard_break_is_structured_inside_nested_control_flow() {
    let source: Source = Source {
        path: crate::source::SourcePath::from_path(Path::new("story/main.twee")).unwrap(),
        kind: crate::source::SourceKind::Twee,
        content: String::from(":: Start\n<<if true>>\na<br>b\n<</if>>"),
    };
    let sources: [Source; 1] = [source];
    let twee: crate::twee::Story<'_> = crate::twee::Story::build(&sources).unwrap();
    let hir: HirStory<'_> = HirStory::lower(&twee).unwrap();
    let HirBodyKind::If(branches) = &hir.passages[0].body[0].kind else {
        panic!("if 应保留结构化分支");
    };

    let body: &[HirBodyNode<'_>] = &branches.branches[0].body;
    let break_index: usize = body
        .iter()
        .position(|node| matches!(node.kind, HirBodyKind::HardBreak))
        .expect("嵌套正文中的 <br> 应成为 HardBreak");
    assert!(matches!(
        (&body[break_index - 1].kind, &body[break_index + 1].kind),
        (HirBodyKind::Text(before), HirBodyKind::Text(after))
            if before.ends_with('a') && after.starts_with('b')
    ));
}

#[test]
fn i18n_template_exposes_translator_text_and_placeholder_bindings() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                text_node("金币：", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$gold").expect("变量表达式应有效"),
                    )),
                    span: span(2),
                },
            ],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);

    let template: I18nTemplate = catalog.template("zh-CN");

    assert_eq!(template.language(), "zh-CN");
    assert!(template.dictionary().is_empty());
    let entry = template
        .passages()
        .get("p5:Start:body.0")
        .expect("模板应按稳定文本身份收录消息");
    assert_eq!(entry.source(), "金币：{$gold}");
    assert_eq!(entry.text(), "");
    assert_eq!(entry.values().get("$gold").map(String::as_str), Some(""));
}

#[test]
fn nmsg_round_trips_multiline_text_and_value_bindings() {
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::from([(
            String::from("items"),
            BTreeMap::from([(String::from("Sword"), String::from("剑"))]),
        )]),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "Found, \"{$item}\"\nChoose",
                "发现“{$item}”\n选择",
                BTreeMap::from([(String::from("$item"), String::from("items"))]),
            ),
        )]),
    );

    let nmsg: String = template.to_nmsg();
    let imported: I18nTemplate = template
        .apply_nmsg(&nmsg)
        .expect("Core 导出的 NMSG 应可无损导回");

    assert_eq!(imported, template);
    assert!(nmsg.starts_with(":: p5:Start:body.0\n[source]\n"));
}

#[test]
fn nmsg_rejects_unknown_and_duplicate_message_ids() {
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("known"),
            I18nTemplateMessage::new("Source", "", BTreeMap::new()),
        )]),
    );
    let unknown: &str = ":: unknown\n[source]\nSource\n[translation]\n译文";
    let duplicate: &str = ":: known\n[source]\nSource\n[translation]\n译文\n\n:: known\n[source]\nSource\n[translation]\n另一译文";

    assert_eq!(
        template.apply_nmsg(unknown),
        Err(I18nMessageError::UnknownMessage {
            id: String::from("unknown"),
        })
    );
    assert_eq!(
        template.apply_nmsg(duplicate),
        Err(I18nMessageError::DuplicateMessage {
            id: String::from("known"),
        })
    );
}

#[test]
fn nmsg_rejects_invalid_values_syntax() {
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("known"),
            I18nTemplateMessage::new("Source", "", BTreeMap::new()),
        )]),
    );
    let nmsg: &str = ":: known\n[source]\nSource\n[translation]\n译文\n[values]\n$name: items";

    assert!(matches!(
        template.apply_nmsg(nmsg),
        Err(I18nMessageError::InvalidSyntax { line: 7, .. })
    ));
}

#[test]
fn nmsg_requires_exactly_one_blank_line_between_messages() {
    let template: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::new(),
        BTreeMap::from([
            (
                String::from("first"),
                I18nTemplateMessage::new("一", "", BTreeMap::new()),
            ),
            (
                String::from("second"),
                I18nTemplateMessage::new("二", "", BTreeMap::new()),
            ),
        ]),
    );
    let missing: &str =
        ":: first\n[source]\n一\n[translation]\n译\n:: second\n[source]\n二\n[translation]\n译";

    assert!(matches!(
        template.apply_nmsg(missing),
        Err(I18nMessageError::InvalidSyntax { line: 6, .. })
    ));
}

#[test]
fn i18n_errors_expose_stable_diagnostics_for_host_and_logger() {
    let validation: Diagnostic = I18nValidationError::MissingPlaceholder {
        id: String::from("message"),
        name: String::from("$name"),
    }
    .diagnostic();
    let package: Diagnostic = NlangPackageError::ForbiddenPath {
        path: String::from("scripts/main.js"),
    }
    .diagnostic();
    let nmsg: Diagnostic = I18nMessageError::InvalidSyntax {
        line: 1,
        message: String::from("无效"),
    }
    .diagnostic();

    assert_eq!(validation.code, "i18n.validation.missing_placeholder");
    assert_eq!(package.code, "i18n.package.forbidden_path");
    assert_eq!(nmsg.code, "i18n.nmsg.invalid_syntax");
    assert_eq!(validation.severity, DiagnosticSeverity::Error);
}

#[test]
fn i18n_empty_translation_falls_back_and_changed_source_is_rejected() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![text_node("Default text", 1)],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let empty: I18nTemplate = catalog.template("zh-CN");
    let validated = catalog
        .validate(empty)
        .expect("空目标文本应是合法的默认语言回退");
    let resolved = catalog
        .resolve(&validated, "p5:Start:body.0", &BTreeMap::new())
        .expect("空目标文本应解析默认原文");

    assert_eq!(resolved.text(), "Default text");
    assert_eq!(resolved.origin(), I18nTextOrigin::Default);

    let changed_source: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new("Changed source", "译文", BTreeMap::new()),
        )]),
    );
    assert!(
        catalog
            .validate(changed_source)
            .expect_err("只读 source 被修改时必须拒绝模板")
            .contains(&I18nValidationError::SourceMismatch {
                id: String::from("p5:Start:body.0"),
            })
    );
}

#[test]
fn i18n_validation_rejects_unknown_content_and_changed_placeholders() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                text_node("金币：", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$gold").expect("变量表达式应有效"),
                    )),
                    span: span(2),
                },
            ],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let passages: BTreeMap<String, I18nTemplateMessage> = BTreeMap::from([
        (
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "金币：{$gold}",
                "Gold: {unknown}",
                BTreeMap::from([(String::from("unknown"), String::from("items"))]),
            ),
        ),
        (
            String::from("p5:Start:body.99"),
            I18nTemplateMessage::new("Unknown", "Unknown", BTreeMap::new()),
        ),
    ]);
    let translation: I18nTemplate = I18nTemplate::new("zh_CN", BTreeMap::new(), passages);

    let errors: Vec<I18nValidationError> = catalog
        .validate(translation)
        .expect_err("无效译文必须被拒绝");

    assert!(errors.contains(&I18nValidationError::InvalidLanguageTag {
        language: String::from("zh_CN"),
    }));
    assert!(errors.contains(&I18nValidationError::UnknownMessage {
        id: String::from("p5:Start:body.99"),
    }));
    assert!(errors.contains(&I18nValidationError::MissingPlaceholder {
        id: String::from("p5:Start:body.0"),
        name: String::from("$gold"),
    }));
    assert!(errors.contains(&I18nValidationError::UnknownPlaceholder {
        id: String::from("p5:Start:body.0"),
        name: String::from("unknown"),
    }));
    assert!(errors.contains(&I18nValidationError::UnknownDictionary {
        id: String::from("p5:Start:body.0"),
        placeholder: String::from("unknown"),
        dictionary: String::from("items"),
    }));
}

#[test]
fn i18n_validation_allows_missing_messages_for_default_language_fallback() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![text_node("默认文本", 1)],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let translation: I18nTemplate = I18nTemplate::new("en-US", BTreeMap::new(), BTreeMap::new());

    let validated = catalog
        .validate(translation)
        .expect("缺失消息应由默认语言回退");

    assert_eq!(validated.language(), "en-US");
    assert!(validated.passages().is_empty());
}

#[test]
fn i18n_template_escapes_literal_braces_before_placeholder_validation() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![text_node("集合写作 {a, b}", 1)],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let template: I18nTemplate = catalog.template("zh-CN");

    assert_eq!(
        template
            .passages()
            .get("p5:Start:body.0")
            .map(I18nTemplateMessage::source),
        Some("集合写作 {{a, b}}")
    );
    assert_eq!(template.passages()["p5:Start:body.0"].text(), "");
    assert!(catalog.validate(template).is_ok());
}

#[test]
fn i18n_resolves_translation_dictionary_and_default_fallback() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![
                    text_node("Found ", 1),
                    HirBodyNode {
                        kind: HirBodyKind::Print(HirPrint::Expression(
                            parse("$item").expect("变量表达式应有效"),
                        )),
                        span: span(2),
                    },
                    text_node(".", 3),
                ],
            },
            HirPassage {
                source: &source.path,
                name: "Fallback",
                tags: Vec::new(),
                body: vec![text_node("Default text.", 4)],
            },
        ],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let dictionaries: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::from([(
        String::from("items"),
        BTreeMap::from([(String::from("Iron Sword"), String::from("铁剑"))]),
    )]);
    let passages: BTreeMap<String, I18nTemplateMessage> = BTreeMap::from([(
        String::from("p5:Start:body.0"),
        I18nTemplateMessage::new(
            "Found {$item}.",
            "获得了 {$item}。",
            BTreeMap::from([(String::from("$item"), String::from("items"))]),
        ),
    )]);
    let translation = catalog
        .validate(I18nTemplate::new("zh-CN", dictionaries, passages))
        .expect("译文应通过校验");

    let translated = catalog
        .resolve(
            &translation,
            "p5:Start:body.0",
            &BTreeMap::from([(String::from("$item"), String::from("Iron Sword"))]),
        )
        .expect("目标语言消息应解析");
    let fallback = catalog
        .resolve(&translation, "p8:Fallback:body.0", &BTreeMap::new())
        .expect("缺失译文应回退默认语言");

    assert_eq!(translated.text(), "获得了 铁剑。");
    assert_eq!(translated.origin(), I18nTextOrigin::Translation);
    assert_eq!(fallback.text(), "Default text.");
    assert_eq!(fallback.origin(), I18nTextOrigin::Default);
}

#[test]
fn i18n_resolution_rejects_unknown_message_and_missing_runtime_value() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                text_node("Value: ", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$value").expect("变量表达式应有效"),
                    )),
                    span: span(2),
                },
            ],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let translation = catalog
        .validate(I18nTemplate::new("en-US", BTreeMap::new(), BTreeMap::new()))
        .expect("空译文应使用默认语言");

    assert_eq!(
        catalog.resolve(&translation, "missing", &BTreeMap::new()),
        Err(I18nResolveError::UnknownMessage {
            id: String::from("missing"),
        })
    );
    assert_eq!(
        catalog.resolve(&translation, "p5:Start:body.0", &BTreeMap::new()),
        Err(I18nResolveError::MissingValue {
            id: String::from("p5:Start:body.0"),
            placeholder: String::from("$value"),
        })
    );
}

#[test]
fn i18n_resolution_rejects_translation_validated_by_another_catalog() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![text_node("Same text", 1)],
        }],
    };
    let first: I18nCatalog = I18nCatalog::from_hir(&story);
    let cloned: I18nCatalog = first.clone();
    let rebuilt: I18nCatalog = I18nCatalog::from_hir(&story);
    let translation = first
        .validate(I18nTemplate::new("en", BTreeMap::new(), BTreeMap::new()))
        .expect("译文应通过首个目录校验");

    assert_eq!(
        rebuilt.resolve(&translation, "p5:Start:body.0", &BTreeMap::new()),
        Err(I18nResolveError::DifferentCatalog)
    );
    assert!(
        cloned
            .resolve(&translation, "p5:Start:body.0", &BTreeMap::new())
            .is_ok()
    );
}

#[test]
fn i18n_export_preserves_compatible_translation_and_reports_removed_messages() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![
            HirPassage {
                source: &source.path,
                name: "Start",
                tags: Vec::new(),
                body: vec![text_node("Default start", 1)],
            },
            HirPassage {
                source: &source.path,
                name: "New",
                tags: Vec::new(),
                body: vec![text_node("New text", 2)],
            },
        ],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let previous: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::from([(
            String::from("items"),
            BTreeMap::from([(String::from("Sword"), String::from("剑"))]),
        )]),
        BTreeMap::from([
            (
                String::from("p5:Start:body.0"),
                I18nTemplateMessage::new("Default start", "已有译文", BTreeMap::new()),
            ),
            (
                String::from("p7:Removed:body.0"),
                I18nTemplateMessage::new("Removed source", "不再使用的译文", BTreeMap::new()),
            ),
        ]),
    );

    let exported = catalog
        .export("zh-CN", Some(&previous))
        .expect("同语言模板应可合并");

    assert_eq!(
        exported
            .template()
            .passages()
            .get("p5:Start:body.0")
            .map(I18nTemplateMessage::text),
        Some("已有译文")
    );
    assert_eq!(
        exported
            .template()
            .passages()
            .get("p3:New:body.0")
            .map(I18nTemplateMessage::text),
        Some("")
    );
    assert_eq!(
        exported.template().passages()["p3:New:body.0"].source(),
        "New text"
    );
    assert!(exported.template().dictionary().contains_key("items"));
    assert_eq!(exported.retained(), &[String::from("p5:Start:body.0")]);
    assert_eq!(exported.added(), &[String::from("p3:New:body.0")]);
    assert!(matches!(
        exported.obsolete(),
        [obsolete]
            if obsolete.id() == "p7:Removed:body.0"
                && obsolete.reason() == I18nExportObsoleteReason::Removed
                && obsolete.message().text() == "不再使用的译文"
    ));
    assert!(catalog.validate(exported.template().clone()).is_ok());
}

#[test]
fn i18n_export_keeps_incompatible_previous_message_in_the_obsolete_report() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Start",
            tags: Vec::new(),
            body: vec![
                text_node("Value: ", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$new").expect("新 placeholder 应有效"),
                    )),
                    span: span(2),
                },
            ],
        }],
    };
    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let previous: I18nTemplate = I18nTemplate::new(
        "en",
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(
                "Value: {$old}",
                "Old: {$old}",
                BTreeMap::from([(String::from("$old"), String::new())]),
            ),
        )]),
    );

    let exported = catalog
        .export("en", Some(&previous))
        .expect("不兼容消息应被刷新而非使整次导出失败");

    assert!(exported.retained().is_empty());
    assert_eq!(exported.added(), &[String::from("p5:Start:body.0")]);
    assert_eq!(exported.template().passages()["p5:Start:body.0"].text(), "");
    assert_eq!(
        exported.template().passages()["p5:Start:body.0"].source(),
        "Value: {$new}"
    );
    assert!(matches!(
        exported.obsolete(),
        [obsolete]
            if obsolete.reason() == I18nExportObsoleteReason::Incompatible
                && obsolete.message().text() == "Old: {$old}"
    ));
    assert!(catalog.validate(exported.template().clone()).is_ok());
}
