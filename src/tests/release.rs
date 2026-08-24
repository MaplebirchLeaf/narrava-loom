use std::{fs, path::PathBuf};

use crate::{nar::NarPackage, package_zip, release::build_directory};

#[test]
fn release_embeds_author_styles_in_the_host_resource_namespace() {
    let suffix: u32 = std::process::id();
    let project = PathBuf::from(format!(
        "target/test-projects/release-style-source-{suffix}"
    ));
    let output = PathBuf::from(format!(
        "target/test-projects/release-style-output-{suffix}"
    ));
    let host = PathBuf::from(format!("target/test-projects/release-style-host-{suffix}"));
    for path in [&project, &output] {
        if path.exists() {
            fs::remove_dir_all(path).unwrap();
        }
    }
    fs::create_dir_all(project.join("contents")).unwrap();
    fs::create_dir_all(project.join("styles/theme")).unwrap();
    fs::write(
        project.join("config.toml"),
        "[game]\nid='test.release-style'\nname='Release Style'\nversion='0.1.0'\ndefault_locale='en'\n",
    )
    .unwrap();
    fs::write(project.join("contents/main.twee"), ":: Start\nReady").unwrap();
    fs::write(
        project.join("styles/theme/main.css"),
        "nv-story { background-image: resource(\"images/scene.svg\"); }",
    )
    .unwrap();
    fs::write(&host, b"host").unwrap();

    build_directory(&project, &output, &host).unwrap();
    let files = package_zip::decode(&fs::read(output.join("game.nar")).unwrap(), 16 << 20).unwrap();
    let package = NarPackage::from_files(files).unwrap().validate().unwrap();

    assert_eq!(
        package
            .resources()
            .text("__narrava/styles/theme/main.css")
            .unwrap(),
        Some("nv-story { background-image: resource(\"images/scene.svg\"); }")
    );

    fs::remove_dir_all(project).unwrap();
    fs::remove_dir_all(output).unwrap();
    fs::remove_file(host).unwrap();
}
