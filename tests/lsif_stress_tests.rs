//! LSIF Import Stress Tests
//!
//! Tests LSIF import performance and correctness with large files.

use magellan::lsif;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_lsif_import_small() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("small.lsif");

    // Create small LSIF file (100 symbols)
    create_test_lsif(&lsif_path, 100);

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert_eq!(pkg.package.name, "test-package");
    assert!(pkg.symbol_count > 0);
}

#[test]
fn test_lsif_import_medium() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("medium.lsif");

    // Create medium LSIF file (10k symbols)
    create_test_lsif(&lsif_path, 10_000);

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert_eq!(pkg.package.name, "test-package");
    assert!(pkg.symbol_count >= 10_000);
}

#[test]
fn test_lsif_import_large() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("large.lsif");

    // Create large LSIF file (100k symbols)
    create_test_lsif(&lsif_path, 100_000);

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_ok());

    let pkg = result.unwrap();
    assert_eq!(pkg.package.name, "test-package");
    assert!(pkg.symbol_count >= 100_000);
}

#[test]
fn test_lsif_import_multiple_packages() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple LSIF files
    let lsif1 = temp_dir.path().join("pkg1.lsif");
    let lsif2 = temp_dir.path().join("pkg2.lsif");

    create_test_lsif(&lsif1, 1_000);
    create_test_lsif(&lsif2, 2_000);

    let result1 = lsif::import::import_lsif(&lsif1);
    let result2 = lsif::import::import_lsif(&lsif2);

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let pkg1 = result1.unwrap();
    let pkg2 = result2.unwrap();

    assert_eq!(pkg1.package.name, "test-package");
    assert_eq!(pkg2.package.name, "test-package");
}

#[test]
fn test_lsif_import_invalid_file() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("invalid.lsif");

    // Create invalid LSIF file
    let mut file = File::create(&lsif_path).unwrap();
    writeln!(file, "not valid json").unwrap();

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_err());
}

#[test]
fn test_lsif_import_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("empty.lsif");

    // Create empty LSIF file
    File::create(&lsif_path).unwrap();

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_err());
}

#[test]
fn test_lsif_import_missing_package() {
    let temp_dir = TempDir::new().unwrap();
    let lsif_path = temp_dir.path().join("no_package.lsif");

    // Create LSIF file without package vertex
    let mut file = File::create(&lsif_path).unwrap();
    writeln!(
        file,
        r#"{{"type":"document","id":"d1","label":"document","uri":"test.rs"}}"#
    )
    .unwrap();

    let result = lsif::import::import_lsif(&lsif_path);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("package"));
}

/// Create a test LSIF file with the specified number of symbols
fn create_test_lsif(path: &std::path::Path, symbol_count: usize) {
    let mut file = File::create(path).unwrap();

    // Write package vertex
    writeln!(file, r#"{{"type":"package","id":"p0","label":"package","data":{{"name":"test-package","version":"1.0.0","manager":"cargo"}}}}"#).unwrap();

    // Write document vertex
    writeln!(file, r#"{{"type":"document","id":"d0","label":"document","uri":"/test/main.rs","languageId":"rust"}}"#).unwrap();

    // Write symbol vertices
    for i in 0..symbol_count {
        let symbol_id = format!("s{}", i);
        let range_id = format!("r{}", i);
        let line = i / 10;
        let character = (i % 10) * 5;

        // Symbol vertex
        writeln!(
            file,
            r#"{{"type":"symbol","id":"{}","label":"symbol","kind":"function"}}"#,
            symbol_id
        )
        .unwrap();

        // Range vertex
        writeln!(
            file,
            r#"{{"type":"range","id":"{}","label":"range","range":[{},{},{},{}]}}"#,
            range_id,
            line,
            character,
            line,
            character + 10
        )
        .unwrap();

        // Item edge
        writeln!(file, r#"{{"type":"item","id":"e{}","label":"item","outV":"{}","inVs":["{}"],"document":"d0"}}"#, 
            i, symbol_id, range_id).unwrap();
    }
}
