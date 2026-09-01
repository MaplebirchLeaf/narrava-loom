// i18n.rs 测试分片 02：按用例边界拆分，保持原测试顺序。
// 本文件由上级测试模块直接包含，共享该模块的导入与辅助夹具。
#[test]
fn i18n_export_rejects_merging_a_different_language() {
    let catalog: I18nCatalog = I18nCatalog::default();
    let previous: I18nTemplate = I18nTemplate::new("en", BTreeMap::new(), BTreeMap::new());

    assert_eq!(
        catalog.export("zh-CN", Some(&previous)),
        Err(I18nExportError::LanguageMismatch {
            requested: String::from("zh-CN"),
            previous: String::from("en"),
        })
    );
}

#[test]
fn i18n_catalog_groups_visible_text_and_print_placeholders() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Inventory",
            tags: Vec::new(),
            body: vec![
                text_node("你有", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$gold").expect("变量表达式应有效"),
                    )),
                    span: span(2),
                },
                text_node("枚金币，称号是", 3),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("title()").expect("调用表达式应有效"),
                    )),
                    span: span(4),
                },
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Literal("。")),
                    span: span(5),
                },
            ],
        }],
    };

    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);

    assert_eq!(catalog.messages().len(), 1);
    let message = &catalog.messages()[0];
    assert_eq!(message.id().as_str(), "p9:Inventory:body.0");
    assert_eq!(message.text(), "你有{$gold}枚金币，称号是{value_2}。");
    assert_eq!(message.placeholders().len(), 2);
    assert_eq!(message.placeholders()[0].name(), "$gold");
    assert_eq!(message.placeholders()[0].node_path(), "body.1");
    assert_eq!(message.placeholders()[1].name(), "value_2");
    assert_eq!(message.placeholders()[1].node_path(), "body.3");
}

#[test]
fn i18n_catalog_preserves_static_member_paths_as_placeholder_names() {
    let source: Source = Source::load(
        Path::new("src/tests/fixtures/game"),
        Path::new("story/main.twee"),
    )
    .expect("示例 Source 应可读取");
    let story: HirStory<'_> = HirStory {
        passages: vec![HirPassage {
            source: &source.path,
            name: "Profile",
            tags: Vec::new(),
            body: vec![
                text_node("姓名：", 1),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$hero.profile.name").expect("成员表达式应有效"),
                    )),
                    span: span(2),
                },
                text_node("，频道：", 3),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("setup.build.channel").expect("Setup 成员表达式应有效"),
                    )),
                    span: span(4),
                },
                text_node("，动态：", 5),
                HirBodyNode {
                    kind: HirBodyKind::Print(HirPrint::Expression(
                        parse("$heroes[$selected].name").expect("动态索引表达式应有效"),
                    )),
                    span: span(6),
                },
            ],
        }],
    };

    let catalog: I18nCatalog = I18nCatalog::from_hir(&story);
    let message = &catalog.messages()[0];

    assert_eq!(
        message.text(),
        "姓名：{$hero.profile.name}，频道：{setup.build.channel}，动态：{value_3}"
    );
    assert_eq!(
        message
            .placeholders()
            .iter()
            .map(|placeholder| placeholder.name())
            .collect::<Vec<_>>(),
        vec!["$hero.profile.name", "setup.build.channel", "value_3"]
    );
}

#[test]
fn nlang_manifest_parses_the_minimal_strict_schema() {
    let json: &str = r#"{
        "locale": "zh-CN",
        "fallback": "zh",
        "version": "1.2.0",
        "game": {
            "id": "example.forest",
            "versions": ">=0.1.0, <0.2.0"
        }
    }"#;

    let manifest: NlangManifest =
        NlangManifest::from_json(json).expect("合法最小 manifest 应可解析");

    assert_eq!(manifest.locale(), "zh-CN");
    assert_eq!(manifest.fallback(), Some("zh"));
    assert_eq!(manifest.version().to_string(), "1.2.0");
    assert_eq!(manifest.game().id(), "example.forest");
    assert_eq!(manifest.game().versions().to_string(), ">=0.1.0, <0.2.0");
}

#[test]
fn nlang_manifest_rejects_invalid_locale_and_target_contracts() {
    let invalid_locale: &str = r#"{
        "locale": "zh_CN",
        "version": "1.0.0",
        "game": { "id": "example.forest", "versions": "*" }
    }"#;
    let invalid_target: &str = r#"{
        "locale": "zh-CN",
        "version": "1.0.0",
        "game": { "id": "example forest", "versions": "*" }
    }"#;

    assert_eq!(
        NlangManifest::from_json(invalid_locale),
        Err(NlangManifestError::InvalidLocale {
            locale: String::from("zh_CN"),
        })
    );
    assert!(matches!(
        NlangManifest::from_json(invalid_target),
        Err(NlangManifestError::InvalidGameTarget { .. })
    ));
}

#[test]
fn nlang_install_accepts_only_matching_file_locale_and_game() {
    let manifest: NlangManifest = NlangManifest::from_json(
        r#"{
            "locale": "zh-CN",
            "version": "1.0.0",
            "game": { "id": "example.forest", "versions": ">=0.1.0, <0.2.0" }
        }"#,
    )
    .expect("manifest 应有效");
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");
    let accepted = manifest
        .validate_install("zh-CN", &game)
        .expect("文件 locale 和目标游戏均匹配时应接受安装");

    assert_eq!(accepted.manifest().locale(), "zh-CN");
}

#[test]
fn nlang_install_classifies_locale_and_game_mismatch() {
    let manifest: NlangManifest = NlangManifest::from_json(
        r#"{
            "locale": "zh-CN",
            "version": "1.0.0",
            "game": { "id": "example.forest", "versions": "^0.1" }
        }"#,
    )
    .expect("manifest 应有效");
    let other_game: GameIdentity =
        GameIdentity::new("example.other", "0.1.0").expect("游戏身份应有效");
    assert_eq!(
        manifest.validate_install("en", &other_game),
        Err(NlangInstallError::LocaleMismatch {
            file: String::from("en"),
            manifest: String::from("zh-CN"),
        })
    );
    assert!(matches!(
        manifest.validate_install("zh-CN", &other_game),
        Err(NlangInstallError::IncompatibleGame { .. })
    ));
}

#[test]
fn nlang_manifest_rejects_unknown_fields_and_invalid_fallback() {
    let unknown: &str = r#"{
        "locale": "zh-CN",
        "version": "1.0.0",
        "game": { "id": "example.forest", "versions": "*" },
        "scripts": []
    }"#;
    let invalid_fallback: &str = r#"{
        "locale": "zh-CN",
        "fallback": "zh_CN",
        "version": "1.0.0",
        "game": { "id": "example.forest", "versions": "*" }
    }"#;

    assert!(matches!(
        NlangManifest::from_json(unknown),
        Err(NlangManifestError::Json(error)) if error.kind() == I18nJsonErrorKind::Data
    ));
    assert_eq!(
        NlangManifest::from_json(invalid_fallback),
        Err(NlangManifestError::InvalidFallback {
            fallback: String::from("zh_CN"),
        })
    );
}

#[test]
fn nlang_package_validates_required_files_without_zip_or_filesystem_access() {
    let package: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("resources/banner.nres", vec![1_u8, 2, 3]),
        NlangPackageEntry::new("translations.nmsg", Vec::new()),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
        NlangPackageEntry::new("manifest.json", valid_nlang_manifest_json().into_bytes()),
    ]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");

    let validated = package
        .validate("zh-CN", &game)
        .expect("有效内存条目应组成语言包");

    assert_eq!(validated.manifest().manifest().locale(), "zh-CN");
    assert_eq!(validated.translation().language(), "zh-CN");
    assert!(
        Arc::ptr_eq(
            &validated.shared_translation(),
            &validated.clone().shared_translation()
        ),
        "语言包 clone 必须共享大规模译文表"
    );
    assert_eq!(validated.file("manifest.json"), None);
    assert_eq!(validated.file("translations.nmsg"), None);
    assert_eq!(validated.file("dictionary.json"), None);
    assert_eq!(
        validated.file("resources/banner.nres"),
        Some(&[1_u8, 2, 3][..])
    );
}

#[test]
fn nlang_package_loads_dictionary_json_into_the_translation() {
    let package: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", valid_nlang_manifest_json().into_bytes()),
        NlangPackageEntry::new("translations.nmsg", Vec::new()),
        NlangPackageEntry::new(
            "dictionary.json",
            r#"{"items":{"Iron Sword":"铁剑","Potion":"药水"}}"#
                .as_bytes()
                .to_vec(),
        ),
    ]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");

    let validated: NlangValidatedPackage = package
        .validate("zh-CN", &game)
        .expect("外置字典应进入语言包译文");

    assert_eq!(
        validated.translation().dictionary().get("items"),
        Some(&BTreeMap::from([
            (String::from("Iron Sword"), String::from("铁剑")),
            (String::from("Potion"), String::from("药水")),
        ]))
    );
}

#[test]
fn nlang_package_merges_multiple_message_files_without_retaining_source_bytes() {
    let package: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", valid_nlang_manifest_json().into_bytes()),
        NlangPackageEntry::new("translations.nmsg", Vec::new()),
        NlangPackageEntry::new(
            "messages/prologue.nmsg",
            b":: prologue\n[source]\nHello\n[translation]\nNi hao\n".to_vec(),
        ),
        NlangPackageEntry::new(
            "messages/forest.nmsg",
            b":: forest\n[source]\nForest\n[translation]\nSen lin\n".to_vec(),
        ),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
    ]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");

    let validated: NlangValidatedPackage = package
        .validate("zh-CN", &game)
        .expect("一个语言应允许多个消息文件");

    assert_eq!(validated.translation().passages().len(), 2);
    assert_eq!(validated.file("messages/prologue.nmsg"), None);
    assert_eq!(validated.file("messages/forest.nmsg"), None);
}

#[test]
fn nlang_output_builds_a_deterministic_binding_ready_file_list() {
    let manifest: NlangManifest =
        NlangManifest::from_json(&valid_nlang_manifest_json()).expect("导出 manifest 应有效");
    let translation: I18nTemplate = I18nTemplate::new(
        "zh-CN",
        BTreeMap::from([(
            String::from("items"),
            BTreeMap::from([(String::from("Iron Sword"), String::from("铁剑"))]),
        )]),
        BTreeMap::new(),
    );

    let output: NlangPackageOutput =
        NlangPackageOutput::build(&manifest, &translation).expect("语言包清单应可导出");

    assert_eq!(output.file_name(), "zh-CN.nlang");
    assert_eq!(
        output
            .entries()
            .iter()
            .map(NlangPackageEntry::path)
            .collect::<Vec<&str>>(),
        vec!["dictionary.json", "manifest.json", "translations.nmsg"]
    );
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");
    let installed: NlangValidatedPackage = NlangPackageInput::new(output.into_entries())
        .validate("zh-CN", &game)
        .expect("导出清单应能原样交给安装边界");
    assert_eq!(installed.translation(), &translation);
}

#[test]
fn nlang_output_rejects_mismatched_locale() {
    let manifest: NlangManifest =
        NlangManifest::from_json(&valid_nlang_manifest_json()).expect("导出 manifest 应有效");
    let wrong_locale: I18nTemplate = I18nTemplate::new("en", BTreeMap::new(), BTreeMap::new());
    assert_eq!(
        NlangPackageOutput::build(&manifest, &wrong_locale),
        Err(NlangPackageOutputError::LocaleMismatch {
            manifest: String::from("zh-CN"),
            translation: String::from("en"),
        })
    );
}

#[test]
fn nlang_package_rejects_invalid_dictionary_json() {
    let package: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", valid_nlang_manifest_json().into_bytes()),
        NlangPackageEntry::new("translations.nmsg", Vec::new()),
        NlangPackageEntry::new("dictionary.json", br#"{"items":[]}"#.to_vec()),
    ]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.5").expect("游戏身份应有效");

    assert!(matches!(
        package.validate("zh-CN", &game),
        Err(NlangPackageError::Dictionary { .. })
    ));
}

#[test]
fn nlang_package_rejects_unsafe_duplicate_and_forbidden_paths() {
    let traversal: NlangPackageInput = NlangPackageInput::new(vec![NlangPackageEntry::new(
        "resources/../scripts/main.js",
        Vec::new(),
    )]);
    let duplicate: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", Vec::new()),
        NlangPackageEntry::new("manifest.json", Vec::new()),
    ]);
    let script: NlangPackageInput = NlangPackageInput::new(vec![NlangPackageEntry::new(
        "resources/main.js",
        Vec::new(),
    )]);
    let l10n: NlangPackageInput =
        NlangPackageInput::new(vec![NlangPackageEntry::new("l10n.json", Vec::new())]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.0").expect("游戏身份应有效");

    assert_eq!(
        traversal.validate("zh-CN", &game),
        Err(NlangPackageError::InvalidPath {
            path: String::from("resources/../scripts/main.js"),
        })
    );
    assert_eq!(
        duplicate.validate("zh-CN", &game),
        Err(NlangPackageError::DuplicatePath {
            path: String::from("manifest.json"),
        })
    );
    assert_eq!(
        script.validate("zh-CN", &game),
        Err(NlangPackageError::ForbiddenPath {
            path: String::from("resources/main.js"),
        })
    );
    assert_eq!(
        l10n.validate("zh-CN", &game),
        Err(NlangPackageError::ForbiddenPath {
            path: String::from("l10n.json"),
        })
    );
}

#[test]
fn nlang_package_requires_unique_utf8_manifest_and_translation_files() {
    let missing: NlangPackageInput = NlangPackageInput::new(vec![NlangPackageEntry::new(
        "manifest.json",
        valid_nlang_manifest_json().into_bytes(),
    )]);
    let invalid_utf8: NlangPackageInput = NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", vec![0xff]),
        NlangPackageEntry::new("translations.nmsg", Vec::new()),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
    ]);
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.0").expect("游戏身份应有效");

    assert_eq!(
        missing.validate("zh-CN", &game),
        Err(NlangPackageError::MissingFile {
            path: String::from("translations.nmsg"),
        })
    );
    assert_eq!(
        invalid_utf8.validate("zh-CN", &game),
        Err(NlangPackageError::InvalidUtf8 {
            path: String::from("manifest.json"),
        })
    );
}

fn text_node(text: &'static str, start: usize) -> HirBodyNode<'static> {
    HirBodyNode {
        kind: HirBodyKind::Text(text),
        span: span(start),
    }
}

fn span(start: usize) -> Span {
    Span {
        start,
        end: start + 1,
        line: 1,
        column: start + 1,
    }
}

fn valid_nlang_manifest_json() -> String {
    String::from(
        r#"{
            "locale": "zh-CN",
            "version": "1.0.0",
            "game": { "id": "example.forest", "versions": ">=0.1.0, <0.2.0" }
        }"#,
    )
}

#[test]
fn i18n_language_chain_uses_declared_fallback_before_default_text() {
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
    let chain: I18nLanguageChain = I18nLanguageChain::validate(
        &catalog,
        "en",
        vec![
            nlang_package("zh-Hant", Some("zh"), "默认文本", ""),
            nlang_package("zh", None, "默认文本", "后备文本"),
        ],
    )
    .expect("声明完整的语言回退链应有效");

    let resolved = chain
        .resolve(&catalog, "p5:Start:body.0", &BTreeMap::new())
        .expect("应从 fallback 解析文本");

    assert_eq!(chain.primary_language(), "zh-Hant");
    assert_eq!(resolved.text(), "后备文本");
    assert_eq!(resolved.origin(), I18nTextOrigin::Translation);
}

#[test]
fn i18n_language_chain_rejects_a_missing_declared_fallback() {
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

    assert_eq!(
        I18nLanguageChain::validate(
            &catalog,
            "en",
            vec![nlang_package("zh-Hant", Some("zh"), "默认文本", "繁體文本",)],
        ),
        Err(I18nLanguageChainError::MissingFallbackPackage {
            locale: String::from("zh-Hant"),
            fallback: String::from("zh"),
        })
    );
}

#[test]
fn i18n_language_chain_selects_manifest_fallbacks_from_unordered_packages() {
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

    let chain: I18nLanguageChain = I18nLanguageChain::select(
        &catalog,
        "en",
        "zh-Hant",
        vec![
            nlang_package("fr", None, "默认文本", "Texte"),
            nlang_package("zh", None, "默认文本", "后备文本"),
            nlang_package("zh-Hant", Some("zh"), "默认文本", ""),
        ],
    )
    .expect("Core 应按 manifest 自动建立目标语言链");

    assert_eq!(
        chain
            .layers()
            .iter()
            .map(|layer| layer.locale())
            .collect::<Vec<&str>>(),
        vec!["zh-Hant", "zh"]
    );
}

#[test]
fn i18n_language_chain_reports_a_missing_primary_package() {
    let catalog: I18nCatalog = I18nCatalog::default();

    assert_eq!(
        I18nLanguageChain::select(&catalog, "en", "zh-CN", Vec::new()),
        Err(I18nLanguageChainError::MissingLanguagePackage {
            locale: String::from("zh-CN"),
        })
    );
}

#[test]
fn runtime_language_uses_none_for_the_game_default_locale() {
    let catalog: I18nCatalog = I18nCatalog::default();

    assert_eq!(
        I18nRuntimeLanguage::select(&catalog, "en", "en", Vec::new()),
        Ok(None)
    );
}

fn nlang_package(
    locale: &str,
    fallback: Option<&str>,
    source: &str,
    text: &str,
) -> NlangValidatedPackage {
    let fallback_field: String = fallback
        .map(|value: &str| format!(", \"fallback\": {value:?}"))
        .unwrap_or_default();
    let manifest: String = format!(
        r#"{{
            "locale": {locale:?}{fallback_field},
            "version": "1.0.0",
            "game": {{ "id": "example.forest", "versions": "*" }}
        }}"#
    );
    let translation: I18nTemplate = I18nTemplate::new(
        locale,
        BTreeMap::new(),
        BTreeMap::from([(
            String::from("p5:Start:body.0"),
            I18nTemplateMessage::new(source, text, BTreeMap::new()),
        )]),
    );
    let translations: String = translation.to_nmsg();
    let game: GameIdentity = GameIdentity::new("example.forest", "0.1.0").expect("游戏身份应有效");
    NlangPackageInput::new(vec![
        NlangPackageEntry::new("manifest.json", manifest.into_bytes()),
        NlangPackageEntry::new("translations.nmsg", translations.into_bytes()),
        NlangPackageEntry::new("dictionary.json", br#"{}"#.to_vec()),
    ])
    .validate(locale, &game)
    .expect("测试语言包应有效")
}
