//! ProjectConfig 行为测试。

use std::path::Path;

use crate::config::{
    ConfigError, GameCompatibility, GameCompatibilityError, GameIdentity, GameIdentityError,
    ProjectConfig,
};

#[test]
fn loads_project_config() {
    let config: ProjectConfig =
        ProjectConfig::load(Path::new("src/tests/fixtures/game")).expect("示例配置应可读取");

    assert_eq!(config.game.id, "example.forest");
    assert_eq!(config.game.name, "Forest");
    assert_eq!(config.game.version, "0.1.0");
    assert_eq!(config.game.default_locale, "zh-CN");
}

#[test]
fn rejects_game_id_with_whitespace() {
    let content: &str = "[game]\nid = 'example forest'\nname = 'Forest'\nversion = '0.1.0'\ndefault_locale = 'zh-CN'";
    let result: Result<ProjectConfig, ConfigError> =
        ProjectConfig::parse(Path::new("config.toml"), content);

    assert_invalid_field(result, "game.id");
}

#[test]
fn rejects_empty_game_name() {
    let content: &str =
        "[game]\nid = 'example.forest'\nname = ''\nversion = '0.1.0'\ndefault_locale = 'zh-CN'";
    let result: Result<ProjectConfig, ConfigError> =
        ProjectConfig::parse(Path::new("config.toml"), content);

    assert_invalid_field(result, "game.name");
}

#[test]
fn rejects_invalid_game_version() {
    let content: &str = "[game]\nid = 'example.forest'\nname = 'Forest'\nversion = 'version-one'\ndefault_locale = 'zh-CN'";
    let result: Result<ProjectConfig, ConfigError> =
        ProjectConfig::parse(Path::new("config.toml"), content);

    assert_invalid_field(result, "game.version");
}

#[test]
fn rejects_empty_default_locale() {
    let content: &str =
        "[game]\nid = 'example.forest'\nname = 'Forest'\nversion = '0.1.0'\ndefault_locale = ''";
    let result: Result<ProjectConfig, ConfigError> =
        ProjectConfig::parse(Path::new("config.toml"), content);

    assert_invalid_field(result, "game.default_locale");
}

#[test]
fn rejects_malformed_default_locale() {
    let content: &str = "[game]\nid = 'example.forest'\nname = 'Forest'\nversion = '0.1.0'\ndefault_locale = 'zh_CN'";
    let result: Result<ProjectConfig, ConfigError> =
        ProjectConfig::parse(Path::new("config.toml"), content);

    assert_invalid_field(result, "game.default_locale");
}

#[test]
fn game_identity_parses_one_shared_id_and_semantic_version_contract() {
    let identity: GameIdentity =
        GameIdentity::new("example.forest", "1.2.3-beta.1").expect("合法游戏身份应可建立");

    assert_eq!(identity.id(), "example.forest");
    assert_eq!(identity.version().to_string(), "1.2.3-beta.1");
}

#[test]
fn game_identity_classifies_invalid_id_and_version() {
    assert_eq!(
        GameIdentity::new("example forest", "1.0.0"),
        Err(GameIdentityError::InvalidId)
    );
    assert!(matches!(
        GameIdentity::new("example.forest", "version-one"),
        Err(GameIdentityError::InvalidVersion { .. })
    ));
}

#[test]
fn project_config_exposes_the_same_validated_game_identity() {
    let config: ProjectConfig =
        ProjectConfig::load(Path::new("src/tests/fixtures/game")).expect("示例配置应可读取");

    let identity: GameIdentity = config.identity().expect("已验证配置应产生游戏身份");

    assert_eq!(identity.id(), config.game.id);
    assert_eq!(identity.version().to_string(), config.game.version);
}

#[test]
fn game_compatibility_matches_case_sensitive_id_and_version_requirement() {
    let target: GameCompatibility =
        GameCompatibility::new("example.forest", ">=1.2, <2.0").expect("合法目标约束应可建立");
    let compatible: GameIdentity =
        GameIdentity::new("example.forest", "1.9.0").expect("游戏身份应有效");
    let wrong_case: GameIdentity =
        GameIdentity::new("Example.Forest", "1.9.0").expect("游戏身份应有效");
    let wrong_version: GameIdentity =
        GameIdentity::new("example.forest", "2.0.0").expect("游戏身份应有效");

    assert!(target.matches(&compatible));
    assert!(!target.matches(&wrong_case));
    assert!(!target.matches(&wrong_version));
}

#[test]
fn game_compatibility_rejects_invalid_target_contracts() {
    assert_eq!(
        GameCompatibility::new("example forest", "*"),
        Err(GameCompatibilityError::InvalidId)
    );
    assert!(matches!(
        GameCompatibility::new("example.forest", "not a requirement"),
        Err(GameCompatibilityError::InvalidVersionRequirement { .. })
    ));
}

fn assert_invalid_field(result: Result<ProjectConfig, ConfigError>, expected: &str) {
    let error: ConfigError = match result {
        Ok(_) => panic!("无效配置不应通过"),
        Err(error) => error,
    };
    let ConfigError::Invalid { field, .. } = error else {
        panic!("应返回字段验证错误");
    };

    assert_eq!(field, expected);
}
