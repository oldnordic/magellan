use super::*;
use std::fs;

#[test]
fn cargo_manifest_detect_include_paths_extracts_target_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "test-crate"

[[bin]]
name = "mybin"
path = "src/main.rs"

[[test]]
name = "integration"
path = "tests/integration.rs"

[[bench]]
name = "perf"
path = "benches/perf.rs"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
    assert!(paths.contains(&"benches/".to_string()));
}

#[test]
fn cargo_manifest_detect_include_paths_with_examples() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "test-crate"

[[example]]
name = "demo"
path = "examples/demo.rs"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"examples/".to_string()));
}

#[test]
fn cargo_manifest_detect_include_paths_with_lib_path() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "test-crate"

[lib]
path = "lib/my_crate.rs"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"lib/".to_string()));
}

#[test]
fn cargo_manifest_parse_extracts_examples_and_lib() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "test-crate"

[lib]
path = "src/lib.rs"

[[example]]
name = "basic"
path = "examples/basic.rs"

[[example]]
name = "advanced"
path = "examples/advanced/main.rs"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();

    assert!(manifest.targets.contains(&"src/lib.rs".to_string()));
    assert!(manifest.targets.contains(&"examples/basic.rs".to_string()));
    assert!(manifest
        .targets
        .contains(&"examples/advanced/main.rs".to_string()));
}

#[test]
fn cargo_manifest_detect_include_paths_no_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "test-crate"

[[bin]]
name = "a"
path = "src/bin/a.rs"

[[bin]]
name = "b"
path = "src/bin/b.rs"

[[test]]
name = "t1"
path = "tests/t1.rs"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert_eq!(paths.iter().filter(|p| *p == "src/").count(), 1);
    assert_eq!(paths.iter().filter(|p| *p == "tests/").count(), 1);
}

#[test]
fn cargo_manifest_detect_include_paths_empty_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let cargo_toml = r#"
[package]
name = "minimal"
"#;
    fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

    let manifest = CargoManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert_eq!(paths, vec!["src/".to_string()]);
}

#[test]
fn pyproject_manifest_parse_minimal() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"
version = "0.1.0"
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.package_name, Some("my-pkg".to_string()));
    assert!(manifest.packages.is_empty());
    assert!(manifest.test_dirs.is_empty());
}

#[test]
fn pyproject_manifest_parse_setuptools_packages() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.packages, vec!["src".to_string()]);
}

#[test]
fn pyproject_manifest_parse_pytest_testpaths() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"

[tool.pytest.ini_options]
testpaths = ["tests", "integration"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    assert_eq!(
        manifest.test_dirs,
        vec!["tests".to_string(), "integration".to_string()]
    );
}

#[test]
fn pyproject_manifest_detect_include_paths() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
}

#[test]
fn pyproject_manifest_detect_include_paths_no_config() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();
    assert!(paths.contains(&"src/".to_string()));
}

#[test]
fn pyproject_manifest_detect_include_paths_flat_layout() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools]
packages = ["my_pkg", "my_pkg.utils"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let manifest = PyprojectManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();
    assert!(!paths.is_empty());
}

#[test]
fn go_module_manifest_parse() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module github.com/example/myapp\n\ngo 1.22\n",
    )
    .unwrap();

    let manifest = GoModuleManifest::parse(dir.path()).unwrap();
    assert_eq!(
        manifest.module_name,
        Some("github.com/example/myapp".to_string())
    );
}

#[test]
fn go_module_manifest_detect_convention_dirs() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.com/app\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("cmd")).unwrap();
    fs::create_dir(dir.path().join("internal")).unwrap();

    let manifest = GoModuleManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths(dir.path());

    assert!(paths.contains(&"cmd/".to_string()));
    assert!(paths.contains(&"internal/".to_string()));
}

#[test]
fn go_module_manifest_no_convention_dirs_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.com/app\n\ngo 1.22\n",
    )
    .unwrap();

    let manifest = GoModuleManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths(dir.path());

    assert_eq!(paths, vec!["src/".to_string()]);
}

#[test]
fn package_json_manifest_parse() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = r#"{"name": "my-lib", "main": "src/index.js", "files": ["dist/", "README.md"]}"#;
    fs::write(dir.path().join("package.json"), pkg).unwrap();

    let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.name, Some("my-lib".to_string()));
    assert_eq!(manifest.main, Some("src/index.js".to_string()));
    assert_eq!(manifest.files, vec!["dist/", "README.md"]);
}

#[test]
fn package_json_manifest_detect_include_paths_from_main() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = r#"{"name": "my-lib", "main": "lib/index.js"}"#;
    fs::write(dir.path().join("package.json"), pkg).unwrap();

    let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"lib/".to_string()));
}

#[test]
fn package_json_manifest_detect_include_paths_from_files() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = r#"{"name": "my-lib", "files": ["src/", "dist/"]}"#;
    fs::write(dir.path().join("package.json"), pkg).unwrap();

    let manifest = PackageJsonManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"dist/".to_string()));
}

#[test]
fn tsconfig_manifest_parse_include() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = r#"{"include": ["src/**/*", "tests/**/*"], "exclude": ["node_modules"]}"#;
    fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

    let manifest = TsconfigManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.include, vec!["src/**/*", "tests/**/*"]);
}

#[test]
fn tsconfig_manifest_detect_include_paths() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = r#"{"include": ["src/**/*", "tests/**/*.ts"]}"#;
    fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

    let manifest = TsconfigManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
}

#[test]
fn tsconfig_manifest_detect_include_paths_dot_slash() {
    let dir = tempfile::tempdir().unwrap();
    let tsconfig = r#"{"include": ["./src/", "./lib/"]}"#;
    fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

    let manifest = TsconfigManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"lib/".to_string()));
}

#[test]
fn maven_manifest_parse() {
    let dir = tempfile::tempdir().unwrap();
    let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>1.0</version>
</project>"#;
    fs::write(dir.path().join("pom.xml"), pom).unwrap();

    let manifest = MavenManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.group_id, Some("com.example".to_string()));
    assert_eq!(manifest.artifact_id, Some("my-app".to_string()));
}

#[test]
fn maven_manifest_detect_include_paths() {
    let dir = tempfile::tempdir().unwrap();
    let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
    fs::write(dir.path().join("pom.xml"), pom).unwrap();
    fs::create_dir_all(dir.path().join("src/main/java")).unwrap();
    fs::create_dir_all(dir.path().join("src/test/java")).unwrap();

    let manifest = MavenManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths(dir.path());

    assert!(paths.contains(&"src/main/java/".to_string()));
    assert!(paths.contains(&"src/test/java/".to_string()));
}

#[test]
fn maven_manifest_no_dirs_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
    fs::write(dir.path().join("pom.xml"), pom).unwrap();

    let manifest = MavenManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths(dir.path());

    assert_eq!(paths, vec!["src/".to_string()]);
}

#[test]
fn cmake_manifest_parse() {
    let dir = tempfile::tempdir().unwrap();
    let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)

add_subdirectory(src)
add_subdirectory(tests)
add_subdirectory(lib)
"#;
    fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

    let manifest = CMakeManifest::parse(dir.path()).unwrap();
    assert_eq!(manifest.project_name, Some("MyProject".to_string()));
    assert_eq!(manifest.subdirectories, vec!["src", "tests", "lib"]);
}

#[test]
fn cmake_manifest_detect_include_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
add_subdirectory(src)
add_subdirectory(include)
"#;
    fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

    let manifest = CMakeManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"include/".to_string()));
}

#[test]
fn cmake_manifest_no_subdirs_falls_back() {
    let dir = tempfile::tempdir().unwrap();
    let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
"#;
    fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

    let manifest = CMakeManifest::parse(dir.path()).unwrap();
    let paths = manifest.detect_include_paths();

    assert_eq!(paths, vec!["src/".to_string()]);
}

#[test]
fn detect_include_paths_prefers_cargo_over_pyproject() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        r#"[package]
name = "rust-project"

[[test]]
name = "t"
path = "tests/t.rs"
"#,
    )
    .unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        r#"[project]
name = "py-part"

[tool.setuptools.packages.find]
where = ["python_src"]
"#,
    )
    .unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
}

#[test]
fn detect_include_paths_falls_back_to_pyproject() {
    let dir = tempfile::tempdir().unwrap();
    let pyproject = r#"
[project]
name = "my-pkg"

[tool.setuptools.packages.find]
where = ["src"]

[tool.pytest.ini_options]
testpaths = ["tests"]
"#;
    fs::write(dir.path().join("pyproject.toml"), pyproject).unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
}

#[test]
fn detect_include_paths_go_module() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("go.mod"),
        "module example.com/app\n\ngo 1.22\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("cmd")).unwrap();
    fs::create_dir(dir.path().join("pkg")).unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"cmd/".to_string()));
    assert!(paths.contains(&"pkg/".to_string()));
}

#[test]
fn detect_include_paths_package_json() {
    let dir = tempfile::tempdir().unwrap();
    let pkg = r#"{"name": "my-lib", "main": "src/index.js", "files": ["dist/"]}"#;
    fs::write(dir.path().join("package.json"), pkg).unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"dist/".to_string()));
}

#[test]
fn detect_include_paths_package_json_with_tsconfig() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("package.json"), r#"{"name": "my-ts-lib"}"#).unwrap();
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"include": ["src/", "tests/"]}"#,
    )
    .unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"tests/".to_string()));
}

#[test]
fn detect_include_paths_maven() {
    let dir = tempfile::tempdir().unwrap();
    let pom = r#"<?xml version="1.0"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
</project>"#;
    fs::write(dir.path().join("pom.xml"), pom).unwrap();
    fs::create_dir_all(dir.path().join("src/main/java")).unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/main/java/".to_string()));
}

#[test]
fn detect_include_paths_cmake() {
    let dir = tempfile::tempdir().unwrap();
    let cmake = r#"cmake_minimum_required(VERSION 3.20)
project(MyProject)
add_subdirectory(src)
add_subdirectory(lib)
"#;
    fs::write(dir.path().join("CMakeLists.txt"), cmake).unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert!(paths.contains(&"src/".to_string()));
    assert!(paths.contains(&"lib/".to_string()));
}

#[test]
fn detect_include_paths_no_manifest_returns_src_default() {
    let dir = tempfile::tempdir().unwrap();

    let paths = detect_include_paths_from_root(dir.path());

    assert_eq!(paths, vec!["src/".to_string()]);
}
